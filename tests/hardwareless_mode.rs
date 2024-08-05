use odyssey::{self, configuration::Configuration};
use simple_logger::SimpleLogger;
use tokio::runtime::{Builder, Runtime};

mod common;

#[test]
fn it_adds_two() {
    assert_eq!(4, 2+2);
}


fn build_runtime() -> Runtime {
    Builder::new_multi_thread()
        .worker_threads(4)
        .thread_name("odyssey-worker")
        .thread_stack_size(3 * 1024 * 1024)
        .enable_time()
        .enable_io()
        .build()
        .expect("Unable to start Tokio runtime")
}
/* 
fn hardwareless_config() -> Configuration {
    Configuration {
        printer {

        }
    }
}*/