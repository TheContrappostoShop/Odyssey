use std::{
    ffi::OsStr,
    fs::File,
    io::{Read, Write},
    ops::Deref,
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, UNIX_EPOCH},
};

use glob::glob;
use itertools::Itertools;
use poem::{
    error::{
        MethodNotAllowedError, NotFoundError, NotImplemented, ServiceUnavailable, Unauthorized,
    },
    get, handler,
    listener::TcpListener,
    post,
    web::{Data, Multipart, Path as URLPath, Query},
    EndpointExt, Result, Route, Server,
};
use poem_openapi::{
    payload::{Binary, Json},
    types::{Any, ToJSON, Type},
    Enum, NewType, Object, OpenApi, OpenApiService, ResponseContent,
};
use serde::{Deserialize, Serialize};
use tokio::{
    sync::{broadcast, mpsc, RwLock},
    time::interval,
};

use crate::{
    configuration::ApiConfig,
    printer::{Operation, PhysicalState, PrinterState},
    printfile::{FileData, LocationCategory, PrintFile, PrintMetadata},
    sl1::Sl1,
};

struct Api;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ZControl {
    z: f32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CureControl {
    cure: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, Object)]
pub struct FilesResponse {
    pub files: Vec<PrintMetadataResponse>,
    pub dirs: Vec<FileDataResponse>,
    pub next_index: Option<usize>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PageParams {
    page_index: usize,
    page_size: usize,
}

const DEFAULT_PAGE_INDEX: usize = 0;
const DEFAULT_PAGE_SIZE: usize = 100;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LocationParams {
    location: LocationCategory,
    subdirectory: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FileParams {
    location: Option<LocationCategory>,
    file_path: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, Object)]
pub struct PrintingStatus {
    print_data: PrintMetadataResponse,
    paused: bool,
    layer: usize,
    physical_state: PhysicalStateResponse,
}

#[derive(Clone, Debug, Serialize, Deserialize, Object)]
pub struct PrinterStateResponse {
    print_data: Option<PrintMetadataResponse>,
    paused: Option<bool>,
    layer: Option<usize>,
    physical_state: Option<PhysicalStateResponse>,
}
impl PrinterStateResponse {
    fn from_printerstate(state: PrinterState) -> PrinterStateResponse {
        match state {
            PrinterState::Printing {
                print_data,
                paused,
                layer,
                physical_state,
            } => PrinterStateResponse {
                print_data: Some(PrintMetadataResponse::from_printmetadata(print_data)),
                paused: Some(paused),
                layer: Some(layer),
                physical_state: Some(PhysicalStateResponse {
                    z: physical_state.z,
                    curing: physical_state.curing,
                }),
            },
            PrinterState::Idle { physical_state } => PrinterStateResponse {
                print_data: None,
                paused: None,
                layer: None,
                physical_state: Some(PhysicalStateResponse {
                    z: physical_state.z,
                    curing: physical_state.curing,
                }),
            },
            PrinterState::Shutdown {} => PrinterStateResponse {
                print_data: None,
                paused: None,
                layer: None,
                physical_state: None,
            },
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, Object)]
pub struct PrintMetadataResponse {
    pub file_data: FileDataResponse,
    pub used_material: f32,
    pub print_time: f32,
    pub layer_height: f32,
    pub layer_count: usize,
}
impl PrintMetadataResponse {
    fn from_printmetadata(printmetadata: PrintMetadata) -> PrintMetadataResponse {
        PrintMetadataResponse {
            file_data: FileDataResponse::from_filedata(printmetadata.file_data),
            used_material: printmetadata.used_material,
            print_time: printmetadata.print_time,
            layer_height: printmetadata.layer_height,
            layer_count: printmetadata.layer_count,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, Object)]
pub struct FileDataResponse {
    pub path: String,
    pub name: String,
    pub last_modified: Option<String>,
    pub location_category: String,
    pub parent_path: String,
}
impl FileDataResponse {
    fn from_filedata(filedata: FileData) -> FileDataResponse {
        FileDataResponse {
            path: filedata.path,
            name: filedata.name,
            last_modified: filedata.last_modified.map(|last| last.to_string()),
            location_category: match filedata.location_category {
                LocationCategory::Local => "Local".to_string(),
                LocationCategory::Usb => "Usb".to_string(),
            },
            parent_path: filedata.parent_path,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, Object)]
pub struct PhysicalStateResponse {
    pub z: f32,
    pub curing: bool,
}

#[OpenApi]
impl Api {
    #[oai(path = "/print/start", method = "post")]
    async fn start_print(
        &self,
        file_params: Result<Query<FileParams>>,
        Data(operation_sender): Data<&mpsc::Sender<Operation>>,
        Data(configuration): Data<&ApiConfig>,
    ) -> Result<()> {
        let file_params = file_params.map(|Query(params)| params)?;

        let location = file_params.location.unwrap_or(LocationCategory::Local);

        let file_path = file_params.file_path;

        let full_file_path = Api::get_file_path(configuration, &file_path, &location)?;

        let file_data = Api::_get_filedata(full_file_path, &location, configuration)?;

        operation_sender
            .send(Operation::StartPrint { file_data })
            .await
            .map_err(ServiceUnavailable)
    }

    #[oai(path = "/print/pause", method = "post")]
    async fn pause_print(
        &self,
        Data(operation_sender): Data<&mpsc::Sender<Operation>>,
    ) -> Result<()> {
        operation_sender
            .send(Operation::PausePrint {})
            .await
            .map_err(ServiceUnavailable)
    }

    #[oai(path = "/print/resume", method = "post")]
    async fn resume_print(
        &self,
        Data(operation_sender): Data<&mpsc::Sender<Operation>>,
    ) -> Result<()> {
        operation_sender
            .send(Operation::ResumePrint {})
            .await
            .map_err(ServiceUnavailable)
    }

    #[oai(path = "/print/cancel", method = "post")]
    async fn cancel_print(
        &self,
        Data(operation_sender): Data<&mpsc::Sender<Operation>>,
    ) -> Result<()> {
        operation_sender
            .send(Operation::StopPrint {})
            .await
            .map_err(ServiceUnavailable)
    }

    #[oai(path = "/shutdown", method = "post")]
    async fn shutdown(&self, Data(operation_sender): Data<&mpsc::Sender<Operation>>) -> Result<()> {
        operation_sender
            .send(Operation::Shutdown {})
            .await
            .map_err(ServiceUnavailable)
    }

    #[oai(path = "/status", method = "get")]
    async fn get_status(
        &self,
        Data(state_ref): Data<&Arc<RwLock<PrinterState>>>,
    ) -> Json<PrinterStateResponse> {
        Json(PrinterStateResponse::from_printerstate(
            state_ref.read().await.clone(),
        ))
    }

    #[oai(path = "/manual", method = "post")]
    async fn manual_control(
        &self,
        z: Result<Query<ZControl>>,
        cure: Result<Query<CureControl>>,
        Data(operation_sender): Data<&mpsc::Sender<Operation>>,
        Data(_state_ref): Data<&Arc<RwLock<PrinterState>>>,
    ) -> Result<()> {
        if let Ok(Query(ZControl { z })) = z {
            operation_sender
                .send(Operation::ManualMove { z })
                .await
                .map_err(ServiceUnavailable)?;
        }

        if let Ok(Query(CureControl { cure })) = cure {
            operation_sender
                .send(Operation::ManualCure { cure })
                .await
                .map_err(ServiceUnavailable)?;
        }

        Ok(())
    }

    #[oai(path = "/files", method = "post")]
    async fn upload_file(&self, mut multipart: Multipart, Data(configuration): Data<&ApiConfig>) {
        log::info!("Uploading file");
        while let Ok(Some(field)) = multipart.next_field().await {
            let file_name = field
                .file_name()
                .map(ToString::to_string)
                .expect("File name not found");
            if let Ok(bytes) = field.bytes().await {
                let mut f = File::create(configuration.upload_path.clone() + "/" + &file_name)
                    .expect("Could not create new file");
                f.write_all(bytes.as_slice())
                    .expect("Failed to write file contents");
            }
        }
    }

    #[oai(path = "/files", method = "get")]
    async fn get_files(
        &self,
        location: Result<Query<LocationParams>>,
        page_params: Result<Query<PageParams>>,
        Data(configuration): Data<&ApiConfig>,
    ) -> Result<Json<FilesResponse>> {
        let location = location.map_or(
            LocationParams {
                location: LocationCategory::Local,
                subdirectory: None,
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

        match location.location {
            LocationCategory::Local => {
                Api::_get_local_files(location.subdirectory, page_params, configuration)
            }
            LocationCategory::Usb => Api::_get_usb_files(page_params, configuration),
        }
    }

    fn _get_local_files(
        subdirectory: Option<String>,
        page_params: PageParams,
        configuration: &ApiConfig,
    ) -> Result<Json<FilesResponse>> {
        let directory = subdirectory.unwrap_or("".to_string());

        if directory.starts_with('/') || directory.starts_with('.') {
            return Err(Unauthorized(MethodNotAllowedError));
        }

        let upload_string = &configuration.upload_path;

        let upload_path = Path::new(upload_string.as_str());
        let full_path = upload_path.join(directory.as_str());

        let read_dir = full_path.read_dir();

        let files_vec = read_dir
            .map_err(|_| NotFoundError)?
            .flatten()
            .map(|f| f.path())
            // TODO add sorting here
            .filter(|f| f.is_dir() || f.extension().and_then(OsStr::to_str).eq(&Some("sl1")));

        let chunks = files_vec.chunks(page_params.page_size);

        let mut chunks_iterator = chunks.into_iter();

        let paths = chunks_iterator
            .nth(page_params.page_index)
            .map_or(Vec::new(), |dirs| dirs.collect_vec());

        let dirs = paths
            .iter()
            .filter(|f| f.is_dir())
            .flat_map(|f| {
                Api::_get_filedata(f.clone(), &LocationCategory::Local, configuration).ok()
            })
            .map(|f| FileDataResponse::from_filedata(f))
            .collect_vec();
        let files = paths
            .iter()
            .filter(|f| !f.is_dir())
            .flat_map(|f| {
                Api::_get_print_metadata(f.clone(), &LocationCategory::Local, configuration).ok()
            })
            .map(|f| PrintMetadataResponse::from_printmetadata(f))
            .collect_vec();

        let next_index =
            Some(page_params.page_index + 1).filter(|_| chunks_iterator.next().is_some());

        Ok(Json(FilesResponse {
            files,
            dirs,
            next_index,
        }))
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

    fn get_file_path(
        configuration: &ApiConfig,
        file_path: &str,
        location: &LocationCategory,
    ) -> Result<PathBuf, NotFoundError> {
        log::info!("Getting full file path {:?}, {:?}", location, file_path);

        match location {
            LocationCategory::Usb => Api::get_usb_file_path(configuration, file_path),
            LocationCategory::Local => Api::get_local_file_path(configuration, file_path),
        }
    }

    // Since USB paths are specified as a glob, find all and filter to file_name
    fn get_usb_file_path(
        configuration: &ApiConfig,
        file_name: &str,
    ) -> Result<PathBuf, NotFoundError> {
        let paths = glob(&configuration.usb_glob).map_err(|_| NotFoundError)?;

        let path_buf = paths
            .filter_map(|path| path.ok())
            .find(|path| path.ends_with(file_name))
            .ok_or_else(|| {
                log::error!("Unable to read USB file");
                NotFoundError
            })?;

        Ok(path_buf)
    }

    // For Local files, look directly for specific file
    fn get_local_file_path(
        configuration: &ApiConfig,
        file_path: &str,
    ) -> Result<PathBuf, NotFoundError> {
        let path = Path::new(&configuration.upload_path).join(file_path);

        if path.exists() {
            Ok(path)
        } else {
            log::error!("Unable to find local file {}", file_path);
            Err(NotFoundError)
        }
    }

    fn _get_filedata(
        target_file: PathBuf,
        location: &LocationCategory,
        configuration: &ApiConfig,
    ) -> Result<FileData> {
        log::info!("Getting file data");
        let modified_time = target_file
            .metadata()
            .ok()
            .and_then(|meta| meta.modified().ok())
            .and_then(|modified| modified.duration_since(UNIX_EPOCH).ok())
            .map(|dur| dur.as_millis());

        // TODO handle USB _get_filedata
        Ok(FileData {
            path: target_file
                .strip_prefix(configuration.upload_path.as_str())
                .map_err(|_| Unauthorized(MethodNotAllowedError))?
                .to_str()
                .map(|path_str| path_str.to_string())
                .ok_or_else(|| {
                    log::error!("Error converting file path");
                    NotFoundError
                })?,
            name: target_file
                .file_name()
                .and_then(|path_str| path_str.to_str())
                .map(|path_str| path_str.to_string())
                .ok_or_else(|| {
                    log::error!("Error converting file name");
                    NotFoundError
                })?,
            last_modified: modified_time,
            location_category: location.clone(),
            parent_path: configuration.upload_path.clone(),
        })
    }

    fn _get_print_metadata(
        target_file: PathBuf,
        location: &LocationCategory,
        configuration: &ApiConfig,
    ) -> Result<PrintMetadata> {
        let file_data = Api::_get_filedata(target_file, location, configuration)?;
        log::info!("Extracting print metadata");

        Ok(Sl1::from_file(file_data).get_metadata())
    }

    #[oai(path = "/file", method = "get")]
    async fn get_file(
        &self,
        file_params: Result<Query<FileParams>>,
        Data(configuration): Data<&ApiConfig>,
    ) -> Result<Binary<Vec<u8>>> {
        let file_params = file_params.map(|Query(params)| params)?;

        let location = file_params.location.unwrap_or(LocationCategory::Local);

        let file_path = file_params.file_path;

        log::info!("Getting file {:?} in {:?}", file_path, location);

        let full_file_path = Api::get_file_path(configuration, &file_path, &location)?;

        let ret = File::open(full_file_path)
            .and_then(|mut file| {
                let mut data: Vec<u8> = vec![];
                file.read_to_end(&mut data).map(|_| data)
            })
            .map_err(|_| NotFoundError);

        ret.map(|vec| Binary(vec)).map_err(|err| err.into())
    }

    #[oai(path = "/file/metadata", method = "get")]
    async fn get_file_metadata(
        &self,
        file_params: Result<Query<FileParams>>,
        Data(configuration): Data<&ApiConfig>,
    ) -> Result<Json<PrintMetadataResponse>> {
        let file_params = file_params.map(|Query(params)| params)?;

        let location = file_params.location.unwrap_or(LocationCategory::Local);

        let file_path = file_params.file_path;

        log::info!(
            "Getting file metadata from {:?} in {:?}",
            file_path,
            location
        );
        let full_file_path = Api::get_file_path(configuration, &file_path, &location)?;

        log::info!("full path: {:?}", full_file_path);

        Ok(Json(PrintMetadataResponse::from_printmetadata(
            Api::_get_print_metadata(full_file_path, &location, configuration)?,
        )))
    }

    #[oai(path = "/file", method = "post")]
    async fn delete_file(
        &self,
        _file_params: Result<Query<FileParams>>,
        Data(_configuration): Data<&ApiConfig>,
    ) -> Result<Json<FileDataResponse>> {
        Err(NotImplemented(MethodNotAllowedError))
    }
}

async fn run_state_listener(
    mut state_receiver: broadcast::Receiver<PrinterState>,
    state_ref: Arc<RwLock<PrinterState>>,
) {
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

pub async fn start_api(
    configuration: ApiConfig,
    operation_sender: mpsc::Sender<Operation>,
    state_receiver: broadcast::Receiver<PrinterState>,
) {
    let state_ref = Arc::new(RwLock::new(PrinterState::Shutdown {}));

    tokio::spawn(run_state_listener(state_receiver, state_ref.clone()));
    /*
        let app = Route::new()
            .at("/status", get(get_status))
            .at("/manual", post(manual_control))
            .at("/print/start", post(start_print))
            .at("/print/cancel", post(cancel_print))
            .at("/print/pause", post(pause_print))
            .at("/print/resume", post(resume_print))
            .at("/shutdown", post(shutdown))
            .at("/files", get(get_files).post(upload_file))
            .at("/file", get(get_file).delete(delete_file))
            .at("/file/metadata", get(get_file_metadata))
            .data(operation_sender)
            .data(state_ref.clone())
            .data(configuration.clone()); //.catch_error(f);
    */
    let port = configuration.port.to_string();
    let addr = format!("0.0.0.0:{port}");

    let api_service = OpenApiService::new(Api, "Odyssey API", "1.0");

    let ui = api_service.swagger_ui();

    let mut app = Route::new().nest("/", api_service);

    if cfg!(debug_assertions) {
        app = app.nest("/docs", ui);
    }

    let app = app
        .data(operation_sender)
        .data(state_ref.clone())
        .data(configuration.clone());

    Server::new(TcpListener::bind(addr))
        .run(app)
        .await
        .expect("Error encountered");
}
