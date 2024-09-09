use odyssey::configuration::Configuration;



pub static TEST_CONFIG: &str = "tests/resources/test.yaml";
pub static CARGO_DIR: &str = env!("CARGO_MANIFEST_DIR");

pub fn default_test_configuration() -> Configuration {
    Configuration::load(CARGO_DIR.to_owned() + "/" + TEST_CONFIG).unwrap()
}