use odyssey;
use simple_logger::SimpleLogger;
use tokio::runtime::{Builder, Runtime};


#[test]
fn it_adds_two() {
    assert_eq!(4, add_two(2));
}