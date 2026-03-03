pub mod cpe;
pub mod types;

pub use cpe::{build_cpe, extract_version, service_to_cpe, service_to_osv, CpeMapping, OsvPackage};
pub use types::{CveRecord, CveReference, CveSeverity, CveSource};
