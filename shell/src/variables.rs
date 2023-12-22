use std::collections::HashMap;

#[derive(Debug)]
pub struct ShellVariable {
    pub value: ShellValue,
    pub exported: bool,
    pub readonly: bool,
}

#[derive(Debug)]
pub enum ShellValue {
    String(String),
    Integer(u64),
    AssociativeArray(HashMap<String, ShellValue>),
    IndexedArray(Vec<String>),
}

impl ShellValue {
    pub fn get_at(&self, index: u32) -> Option<&str> {
        match self {
            ShellValue::String(s) => {
                if index == 0 {
                    Some(s.as_str())
                } else {
                    None
                }
            }
            ShellValue::Integer(_) => todo!("indexing into integer"),
            ShellValue::AssociativeArray(_) => todo!("indexing into associative array"),
            ShellValue::IndexedArray(values) => values.get(index as usize).map(|s| s.as_str()),
        }
    }

    pub fn get_all(&self, _concatenate: bool) -> String {
        // TODO: implement concatenate (or not)
        match self {
            ShellValue::String(s) => s.to_owned(),
            ShellValue::Integer(i) => i.to_string(),
            ShellValue::AssociativeArray(_) => todo!("converting associative array to string"),
            ShellValue::IndexedArray(arr) => arr.join(" "),
        }
    }
}

impl From<&str> for ShellValue {
    fn from(value: &str) -> Self {
        ShellValue::String(value.to_owned())
    }
}

impl From<Vec<String>> for ShellValue {
    fn from(value: Vec<String>) -> Self {
        ShellValue::IndexedArray(value)
    }
}

impl From<&ShellValue> for String {
    fn from(value: &ShellValue) -> Self {
        match value {
            ShellValue::String(s) => s.clone(),
            ShellValue::Integer(i) => i.to_string(),
            ShellValue::AssociativeArray(_) => todo!("converting associative array to string"),
            ShellValue::IndexedArray(arr) => {
                arr.first().map_or_else(|| "".to_owned(), |s| s.clone())
            }
        }
    }
}
