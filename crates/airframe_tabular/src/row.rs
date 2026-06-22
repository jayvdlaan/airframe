use std::collections::BTreeMap;

/// One parsed record, addressed by logical field name from the profile.
#[derive(Debug, Clone)]
pub struct Row {
    line: usize,
    fields: BTreeMap<String, String>,
}

impl Row {
    pub(crate) fn empty(line: usize) -> Self {
        Self {
            line,
            fields: BTreeMap::new(),
        }
    }

    pub(crate) fn set(&mut self, logical: String, value: String) {
        self.fields.insert(logical, value);
    }

    /// Source line number (0-based, excluding skipped/header lines).
    pub fn line(&self) -> usize {
        self.line
    }

    pub fn get(&self, logical: &str) -> Option<&str> {
        self.fields.get(logical).map(String::as_str)
    }

    pub fn fields(&self) -> &BTreeMap<String, String> {
        &self.fields
    }
}
