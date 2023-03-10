use std::sync::{Arc, Mutex};

use poem::{get, handler, listener::TcpListener, web::{Path, Data, Query}, IntoResponse, Route, middleware::AddData, EndpointExt, Server, Middleware, Endpoint, Request, Result};
use tokio::sync::{mpsc, broadcast};

use crate::{configuration::ApiConfig, printer::{Printer, Operation, PrinterState}, gcode::Gcode};

#[handler]
fn hello(Path(name): Path<String>) -> String {
    format!("hello: {}", name)
}



// New strategy: Implement a channel for api->printer communication, keep a listener to it open constantly
#[handler]
async fn start_print(Query(file_name): Query<String>, Data(printer): Data<&Arc<Mutex<Printer<Gcode>>>>) -> String {
    todo!()
}

pub async fn start_api(configuration: ApiConfig, operation_sender: mpsc::Sender<Operation>, state_receiver: broadcast::Receiver<PrinterState>) {
    
    let app = Route::new()
        .at("/hello/:name", get(hello))
        .data(operation_sender);

    let port = configuration.port.to_string();
    let addr = format!("127.0.0.1:{port}");

    Server::new(TcpListener::bind(addr))
        .run(app)
        .await.expect("Error encountered");
    
}

/*
struct PrinterMiddleware {
    printer: Printer<Gcode>
}

impl<E: Endpoint> Middleware<E> for PrinterMiddleware {
    type Output = PrinterMiddlewareImpl<E>;

    fn transform(&self, ep: E) -> Self::Output {
        PrinterMiddlewareImpl {
            ep: ep,
            printer: &mut self.printer,
        }

    }
}

struct PrinterMiddlewareImpl<E> {
    ep: E,
    printer: &mut Printer<Gcode>
}


#[poem::async_trait]
impl<E: Endpoint> Endpoint for PrinterMiddlewareImpl<E> {
    type Output = E::Output;

    async fn call(&self, mut req: Request) -> Result<Self::Output> {
        req.extensions_mut().insert(self.printer);

        self.ep.call(req).await
    }
}*/