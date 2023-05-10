use std::{sync::{Arc, Mutex, atomic::AtomicPtr}, path::Path, fs::File, io::Write, time::Duration};

use itertools::Itertools;
use poem::{
    get, 
    handler, 
    listener::TcpListener, 
    web::{
        Path as URLPath,
        Data,
        Json, Multipart
    }, 
    Route, 
    EndpointExt, 
    Server, 
    post};
use tokio::{sync::{mpsc, broadcast, RwLock}, time::interval};
use glob::glob;

use crate::{configuration::ApiConfig, printer::{Printer, Operation, PrinterState}, gcode::Gcode};

#[handler]
fn hello(URLPath(name): URLPath<String>) -> String {
    format!("hello: {}", name)
}


#[handler]
// GET /print/start/:file_name
async fn start_print(URLPath(file_name): URLPath<String>, Data(operation_sender): Data<&mpsc::Sender<Operation>>, Data(configuration): Data<&ApiConfig>) {
    let file_path = Path::new(&configuration.upload_path.clone())
        .join(file_name).to_str().expect("Error parsing file location").to_string();
    operation_sender.send(Operation::StartPrint { file_name: file_path})
        .await.expect("Error communicating with printer");
    
}

#[handler]
async fn pause_print(Data(operation_sender): Data<&mpsc::Sender<Operation>>) {
    operation_sender.send(Operation::PausePrint { })
        .await.expect("Error communicating with printer");
}

#[handler]
async fn resume_print(Data(operation_sender): Data<&mpsc::Sender<Operation>>) {
    operation_sender.send(Operation::ResumePrint { })
        .await.expect("Error communicating with printer");
}

#[handler]
async fn cancel_print(Data(operation_sender): Data<&mpsc::Sender<Operation>>) {
    operation_sender.send(Operation::StopPrint { })
        .await.expect("Error communicating with printer");
}

#[handler]
async fn get_status(Data(state_ref): Data<&Arc<RwLock<PrinterState>>>) -> Json<PrinterState> {
    poem::web::Json(state_ref.read().await.clone())
}

#[handler]
async fn upload_file(mut multipart: Multipart, Data(configuration): Data<&ApiConfig>) {
    while let Ok(Some(field)) = multipart.next_field().await {
        let file_name = field.file_name().map(ToString::to_string).expect("File name not found");
        if let Ok(bytes) = field.bytes().await {
            let mut f = File::create(configuration.upload_path.clone()+"/"+&file_name).expect("Could not create new file");
            f.write_all(bytes.as_slice()).expect("Failed to write file contents");
        }
    }
}

#[handler]
async fn get_files(Data(configuration): Data<&ApiConfig>) -> Json<Vec<String>> {
    poem::web::Json(glob(&(configuration.upload_path.as_str().to_owned()+"/*"))
        .expect("Failed to parse upload path")
        .map(|result| result.expect("Error reading path"))
        .map(|path| path.into_os_string().into_string().expect("Error parsing path"))
        .collect_vec())
}

#[handler]
async fn get_file(URLPath(file_name): URLPath<String>, Data(configuration): Data<&ApiConfig>) {
    todo!()
}

#[handler]
async fn delete_file(URLPath(file_name): URLPath<String>, Data(configuration): Data<&ApiConfig>) {
    todo!()
}

#[handler]
async fn get_usb_files(Data(configuration): Data<&ApiConfig>) -> Json<Vec<String>> {
    poem::web::Json(glob(&configuration.usb_glob)
        .expect("Failed to read glob pattern")
        .map(|result| result.expect("Error reading path"))
        .map(|path| path.into_os_string().into_string().expect("Error parsing path"))
        .collect_vec())
}

#[handler]
async fn get_usb_file(URLPath(file_name): URLPath<String>, Data(configuration): Data<&ApiConfig>) {
    todo!()
}

async fn run_state_listener(mut state_receiver: broadcast::Receiver<PrinterState>, mut state_ref: Arc<RwLock<PrinterState>>) {
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

pub async fn start_api(configuration: ApiConfig, operation_sender: mpsc::Sender<Operation>, mut state_receiver: broadcast::Receiver<PrinterState>) {
    
    let mut state_ref = Arc::new(RwLock::new(PrinterState::Shutdown));
    
    tokio::spawn(run_state_listener(
        state_receiver,
        state_ref.clone()
    ));

    let app = Route::new()
        .at("/status", get(get_status))
        .at("/print/start/:file_name", post(start_print))
        .at("/print/cancel", post(cancel_print))
        .at("/print/pause", post(pause_print))
        .at("/print/resume", post(resume_print))
        .at("/files", get(get_files).post(upload_file))
        .at("/files/:file_name", get(get_file).delete(delete_file))
        .at("/files/usb", get(get_usb_files))
        .at("/files/usb/:file_name", get(get_usb_file))
        .data(operation_sender)
        .data(state_ref.clone())
        .data(configuration.clone());

    let port = configuration.port.to_string();
    let addr = format!("127.0.0.1:{port}");

    Server::new(TcpListener::bind(addr))
        .run(app)
        .await.expect("Error encountered");
    
}

