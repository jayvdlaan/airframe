use thiserror::Error;

#[derive(Debug, Error)]
pub enum TabularError {
    #[error("CSV read error: {0}")]
    Csv(#[from] csv::Error),

    #[error("unknown encoding label: {0:?}")]
    UnknownEncoding(String),

    #[error("column {0:?} not found in source")]
    MissingColumn(String),

    #[error("failed to parse {kind} value {value:?}: {reason}")]
    Parse {
        kind: &'static str,
        value: String,
        reason: String,
    },
}
