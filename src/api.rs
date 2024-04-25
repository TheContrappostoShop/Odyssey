use std::{sync::Arc, path::{Path, PathBuf}, fs::{File, DirEntry}, io::{Write, Read}, time::{Duration, UNIX_EPOCH}, ffi::OsStr};

use itertools::Itertools;
use poem::{
    error::{MethodNotAllowedError, NotFoundError, NotImplemented, ServiceUnavailable, Unauthorized}, get, handler, listener::TcpListener, post, web::{
        Data, Json, Multipart, Path as URLPath, Query
    }, EndpointExt, Result, Route, Server};
use serde::{Deserialize, Serialize};
use tokio::{sync::{mpsc, broadcast, RwLock}, time::interval};
use glob::glob;

use crate::{configuration::ApiConfig, printer::{Operation, PrinterState}, printfile::{LocationCategory, FileData, PrintMetadata, PrintFile}, sl1::Sl1};

#[handler]
async fn start_print(
    URLPath((location, file_name)): URLPath<(LocationCategory,String)>,
    Data(operation_sender): Data<&mpsc::Sender<Operation>>,
    Data(configuration): Data<&ApiConfig>
) -> Result<()> {
    
    let pathbuf = get_file_path(configuration, file_name.clone(), location.clone())?;

    let path = pathbuf.into_os_string().into_string().map_err(|_| NotFoundError)?;

    let file_data = FileData{
        name: file_name,
        path,
        last_modified: None,
        location_category: location
    };

    operation_sender.send(Operation::StartPrint { file_data }).await
        .map_err(ServiceUnavailable)
}

#[handler]
async fn pause_print(Data(operation_sender): Data<&mpsc::Sender<Operation>>) -> Result<()> {
    operation_sender.send(Operation::PausePrint { }).await
        .map_err(ServiceUnavailable)
}

#[handler]
async fn resume_print(Data(operation_sender): Data<&mpsc::Sender<Operation>>) -> Result<()> {
    operation_sender.send(Operation::ResumePrint { }).await
        .map_err(ServiceUnavailable)
}

#[handler]
async fn cancel_print(Data(operation_sender): Data<&mpsc::Sender<Operation>>) -> Result<()> {
    operation_sender.send(Operation::StopPrint { }).await
        .map_err(ServiceUnavailable)
}

#[handler]
async fn shutdown(Data(operation_sender): Data<&mpsc::Sender<Operation>>) -> Result<()> {
    operation_sender.send(Operation::Shutdown { }).await
        .map_err(ServiceUnavailable)
}

#[handler]
async fn get_status(Data(state_ref): Data<&Arc<RwLock<PrinterState>>>) -> Json<PrinterState> {
    poem::web::Json(state_ref.read().await.clone())
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ZControl {
    z: f32
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CureControl {
    cure: bool
}

#[handler]
async fn manual_control(
    z: Result<Query<ZControl>>,
    cure: Result<Query<CureControl>>,
    Data(operation_sender): Data<&mpsc::Sender<Operation>>,
    Data(_state_ref): Data<&Arc<RwLock<PrinterState>>>
) -> Result<()> {
    if let Ok(Query(ZControl { z })) = z {
        operation_sender.send(Operation::ManualMove { z })
            .await.map_err(ServiceUnavailable)?;
    }
    
    if let Ok(Query(CureControl { cure })) = cure {
        operation_sender.send(Operation::ManualCure { cure })
            .await.map_err(ServiceUnavailable)?;
    }

    Ok(())
}

#[handler]
async fn upload_file(mut multipart: Multipart, Data(configuration): Data<&ApiConfig>) {
    log::info!("Uploading file");
    while let Ok(Some(field)) = multipart.next_field().await {
        let file_name = field.file_name().map(ToString::to_string).expect("File name not found");
        if let Ok(bytes) = field.bytes().await {
            let mut f = File::create(configuration.upload_path.clone()+"/"+&file_name).expect("Could not create new file");
            f.write_all(bytes.as_slice()).expect("Failed to write file contents");
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FilesResponse {
    pub files: Vec<PrintMetadata>,
    pub next_index: Option<usize>
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DirsResponse {
    pub dirs: Vec<FileData>,
    pub next_index: Option<usize>
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PageParams {
    page_index: usize,
    page_size: usize
}

const DEFAULT_PAGE_INDEX: usize = 0;
const DEFAULT_PAGE_SIZE: usize = 100;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LocationParams {
    category: LocationCategory,
    sub_directory: Option<String>,
}

#[handler]
async fn get_files(
    location: Result<Query<LocationParams>>,
    page_params: Result<Query<PageParams>>,
    Data(configuration): Data<&ApiConfig>,
) -> Result<Json<FilesResponse>> {
    let location = location.map_or(
        LocationParams {
            category: LocationCategory::Local,
            sub_directory: None,
        },
        |Query(loc_params)| loc_params,
    );

    let page_params = page_params.map_or(
        PageParams {
            page_index: DEFAULT_PAGE_INDEX,
            page_size: DEFAULT_PAGE_SIZE,
        },
        |Query(params)| params,
    );

    log::info!("Getting files in {:?}, {:?}", location, page_params);

    match location.category {
        LocationCategory::Local => {
            _get_local_files(location.sub_directory, page_params, configuration)
        }
        LocationCategory::Usb => _get_usb_files(page_params, configuration),
    }
}

fn _get_local_files(
    sub_directory: Option<String>,
    page_params: PageParams,
    configuration: &ApiConfig,
) -> Result<Json<FilesResponse>> {
    let directory = sub_directory.unwrap_or("".to_string());
    
    if directory.starts_with('/') || directory.starts_with('.') {
        return Err(Unauthorized(MethodNotAllowedError))
    }

    let upload_string = &configuration.upload_path;

    let upload_path = Path::new(upload_string.as_str());
    let full_path = upload_path.join(directory.as_str());

    let read_dir = full_path.read_dir();

    let files_vec = read_dir
        .map_err(|_| NotFoundError)?
        .flatten()
        .filter(|f| !f.path().is_dir())
        .filter(|f| {
            f.path()
                .extension()
                .and_then(OsStr::to_str)
                .eq(&Some("sl1"))
        });

    let chunks = files_vec.chunks(page_params.page_size);

    let mut chunks_iterator = chunks.into_iter();

    let files = chunks_iterator
        .nth(page_params.page_index)
        .map_or(Vec::new(), |dirs| {
            dirs.flat_map(|f| get_print_metadata(f, LocationCategory::Local).ok())
                .collect_vec()
        });

    let next_index = Some(page_params.page_index + 1).filter(|_| chunks_iterator.next().is_some());

    Ok(Json(FilesResponse { files, next_index }))
}

fn _get_usb_files(
    _page_params: PageParams,
    _configuration: &ApiConfig,
) -> Result<Json<FilesResponse>> {
    Err(NotImplemented(MethodNotAllowedError))

    /*
    poem::web::Json(glob(&configuration.usb_glob)
        .expect("Failed to read glob pattern")
        .map(|result| result.expect("Error reading path"))
        .map(|path| path.into_os_string().into_string().expect("Error parsing path"))
        .collect_vec())
    */
}

#[handler]
async fn get_dirs(
    location: Result<Query<LocationParams>>,
    page_params: Result<Query<PageParams>>,
    Data(configuration): Data<&ApiConfig>,
) -> Result<Json<DirsResponse>> {
    let location = location.map_or(
        LocationParams {
            category: LocationCategory::Local,
            sub_directory: None,
        },
        |Query(loc_params)| loc_params,
    );

    let page_params = page_params.map_or(
        PageParams {
            page_index: DEFAULT_PAGE_INDEX,
            page_size: DEFAULT_PAGE_SIZE,
        },
        |Query(params)| params,
    );

    log::info!("Getting files in {:?}, {:?}", location, page_params);

    match location.category {
        LocationCategory::Local => {
            _get_local_dirs(location.sub_directory, page_params, configuration)
        }
        LocationCategory::Usb => _get_usb_dirs(page_params, configuration),
    }
}
fn _get_local_dirs(
    sub_directory: Option<String>,
    page_params: PageParams,
    configuration: &ApiConfig,
) -> Result<Json<DirsResponse>> {
    let directory = sub_directory.unwrap_or("".to_string());

    if directory.starts_with('/') || directory.starts_with('.') {
        return Err(Unauthorized(MethodNotAllowedError))
    }

    let upload_string = &configuration.upload_path;

    let upload_path = Path::new(upload_string.as_str());
    let full_path = upload_path.join(directory.as_str());

    let read_dir = full_path.read_dir();

    let dirs_vec = read_dir
        .map_err(|_| NotFoundError)?
        .flatten()
        .filter(|f| f.path().is_dir());

    let chunks = dirs_vec.chunks(page_params.page_size);

    let mut chunks_iterator = chunks.into_iter();

    let dirs = chunks_iterator
        .nth(page_params.page_index)
        .map_or(Vec::new(), |dirs| {
            dirs.flat_map(|f| get_filedata(f, LocationCategory::Local).ok())
                .collect_vec()
        });

    let next_index = Some(page_params.page_index + 1).filter(|_| chunks_iterator.next().is_some());

    Ok(Json(DirsResponse { dirs, next_index }))
}

fn _get_usb_dirs(
    _page_params: PageParams,
    _configuration: &ApiConfig,
) -> Result<Json<DirsResponse>> {
    Err(NotImplemented(MethodNotAllowedError))

    /*
    poem::web::Json(glob(&configuration.usb_glob)
        .expect("Failed to read glob pattern")
        .map(|result| result.expect("Error reading path"))
        .map(|path| path.into_os_string().into_string().expect("Error parsing path"))
        .collect_vec())
    */
}

fn get_file_path(configuration: &ApiConfig, file_name: String, location: LocationCategory) -> Result<PathBuf, NotFoundError> {
    log::info!("Getting file path {:?}, {:?}", location, file_name);

    match location {
        LocationCategory::Usb => {
            get_usb_file_path(configuration, file_name)
        },
        LocationCategory::Local => {
            get_local_file_path(configuration, file_name)
        }
    }
}

// Since USB paths are specified as a glob, find all and filter to file_name
fn get_usb_file_path(configuration: &ApiConfig, file_name: String) -> Result<PathBuf, NotFoundError> {
    let paths = glob(&configuration.usb_glob)
        .map_err(|_| NotFoundError)?;

    let path_buf = paths.filter_map(|path| path.ok())
        .find(|path|
            path.ends_with(file_name.clone())
        ).ok_or_else(|| {
            log::error!("Unable to read USB file");
            NotFoundError
        })?;
        
    Ok(path_buf)
}

// For Local files, look directly for specific file
fn get_local_file_path(configuration: &ApiConfig, file_name: String) -> Result<PathBuf, NotFoundError> {
    let path = Path::new(&configuration.upload_path).join(file_name);

    if path.exists() {
        Ok(path)
    }
    else {
        log::error!("Unable to read local file");
        Err(NotFoundError)
    }
}

fn get_filedata(file: DirEntry, location: LocationCategory) -> Result<FileData> {
    log::info!("Getting file data");
    let modified_time = file.metadata().ok()
        .and_then(|meta| meta.modified().ok())
        .and_then(|modified| modified.duration_since(UNIX_EPOCH).ok()).map(|dur| dur.as_millis());

    Ok(FileData {
        path: file.path().into_os_string().into_string().map_err(|_| {
            log::error!("Error converting file path");
            NotFoundError
        })?,
        name: file.file_name().into_string().map_err(|_| {
            log::error!("Error converting file name");
            NotFoundError
        })?,
        last_modified: modified_time,
        location_category: location
    })
}

fn get_print_metadata(file: DirEntry, location: LocationCategory) -> Result<PrintMetadata> {

    let file_data = get_filedata(file, location)?;
    log::info!("Extracting print metadata");

    Ok(Sl1::from_file(file_data).get_metadata())
}

#[handler]
async fn get_file(
    URLPath((location, file_name)): URLPath<(LocationCategory,String)>,
    Data(configuration): Data<&ApiConfig>
) -> Result<Vec<u8>> {
    let file_path = get_file_path(configuration, file_name, location)?;
    
    let ret = File::open(file_path)
        .and_then(|mut file| {
            let mut data: Vec<u8> = vec![];
            file.read_to_end(&mut data)
                .map(|_| data)
        }).map_err(|_| NotFoundError);
        
    ret.map_err(|err| err.into())
}

#[handler]
async fn delete_file(
    URLPath((_location, _file_name)): URLPath<(LocationCategory,String)>,
    Data(_configuration): Data<&ApiConfig>
) -> Result<Json<FileData>> {
    Err(NotImplemented(MethodNotAllowedError))
}

async fn run_state_listener(mut state_receiver: broadcast::Receiver<PrinterState>, state_ref: Arc<RwLock<PrinterState>>) {
    let mut interv = interval(Duration::from_millis(1000));

    let mut state: Result<PrinterState, broadcast::error::TryRecvError>;

    loop {
        state = state_receiver.try_recv();
        if state.is_ok() {
            let mut state_data = state_ref.write().await;
            *state_data = state.clone().unwrap();
        }
        
        interv.tick().await;
    }
}

pub async fn start_api(configuration: ApiConfig, operation_sender: mpsc::Sender<Operation>, state_receiver: broadcast::Receiver<PrinterState>) {
    
    let state_ref = Arc::new(RwLock::new(PrinterState::Shutdown { }));
    
    tokio::spawn(run_state_listener(
        state_receiver,
        state_ref.clone()
    ));

    let app = Route::new()
        .at("/status", get(get_status))
        .at("/manual", post(manual_control))
        .at("/print/start/:location/:file_name", post(start_print))
        .at("/print/cancel", post(cancel_print))
        .at("/print/pause", post(pause_print))
        .at("/print/resume", post(resume_print))
        .at("/shutdown", post(shutdown))
        .at("/files", get(get_files).post(upload_file))
        .at("/files/:location/:file_name", get(get_file).delete(delete_file))
        .at("/dirs", get(get_dirs))
        .data(operation_sender)
        .data(state_ref.clone())
        .data(configuration.clone())
        ;//.catch_error(f);

    let port = configuration.port.to_string();
    let addr = format!("0.0.0.0:{port}");

    Server::new(TcpListener::bind(addr))
        .run(app)
        .await.expect("Error encountered");

}
