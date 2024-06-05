use std::str::FromStr;

use printfile::PrintFile;
use sl1::*;

use crate::api_objects::FileMetadata;

pub mod printfile;
pub mod sl1;
pub mod goo;

pub enum SupportedFileTypes {
    SL1,
    GOO,
}

impl FromStr for SupportedFileTypes {
    type Err = ();

    fn from_str(input: &str) -> Result<SupportedFileTypes, Self::Err> {
        match input.to_lowercase().as_str() {
            "sl1" => Ok(SupportedFileTypes::SL1),
            "goo" => Ok(SupportedFileTypes::GOO),
            _ => Err(()),
        }
    }
}

impl SupportedFileTypes {
    fn get_extension(&self) -> &'static str {
        match self {
            Self::SL1 => "sl1",
            Self::GOO => "goo",
        }
    }

    fn from_file(&self, file_data: FileMetadata) -> impl PrintFile {
        match self {
            Self::SL1 => Sl1::from_file(file_data),
            Self::GOO => Sl1::from_file(file_data),
        }
    }
}
