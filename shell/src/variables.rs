use anyhow::Result;
use rand::Rng;
use std::collections::HashMap;
use std::fmt::Write;

#[derive(Clone, Debug)]
pub struct ShellVariable {
    pub value: ShellValue,
    pub exported: bool,
    pub readonly: bool,
    pub enumerable: bool,
}

impl ShellVariable {
    pub fn export(&mut self) {
        self.exported = true;
    }

    pub fn unexport(&mut self) {
        self.exported = false;
    }

    pub fn set_readonly(&mut self) {
        self.readonly = true;
    }

    pub fn unset_readonly(&mut self) {
        self.readonly = false;
    }
}

#[derive(Clone, Debug)]
pub enum ShellValue {
    String(String),
    Integer(u64),
    AssociativeArray(HashMap<String, ShellValue>),
    IndexedArray(Vec<String>),
    Random,
}

impl ShellValue {
    pub fn format(&self) -> Result<String> {
        match self {
            ShellValue::String(s) => {
                // TODO: Handle embedded newlines and other special chars.
                if s.contains(' ') {
                    Ok(format!("'{s}'"))
                } else {
                    Ok(s.clone())
                }
            }
            ShellValue::Integer(_) => todo!("UNIMPLEMENTED: formatting integers"),
            ShellValue::AssociativeArray(_) => {
                todo!("UNIMPLEMENTED: formatting associative arrays")
            }
            ShellValue::IndexedArray(values) => {
                let mut result = String::new();
                result.push('(');

                for (i, value) in values.iter().enumerate() {
                    if i > 0 {
                        result.push(' ');
                    }
                    write!(result, "[{i}]=\"{value}\"")?;
                }

                result.push(')');
                Ok(result)
            }
            ShellValue::Random => todo!("UNIMPLEMENTED: formatting RANDOM"),
        }
    }

    pub fn get_at(&self, index: u32) -> Option<&str> {
        match self {
            ShellValue::String(s) => {
                if index == 0 {
                    Some(s.as_str())
                } else {
                    None
                }
            }
            ShellValue::Integer(_) => todo!("UNIMPLEMENTED: indexing into integer"),
            ShellValue::AssociativeArray(_) => {
                todo!("UNIMPLEMENTED: indexing into associative array")
            }
            ShellValue::IndexedArray(values) => values.get(index as usize).map(|s| s.as_str()),
            ShellValue::Random => todo!("UNIMPLEMENTED: indexing into RANDOM"),
        }
    }

    pub fn get_all(&self, _concatenate: bool) -> String {
        // TODO: implement concatenate (or not)
        match self {
            ShellValue::String(s) => s.to_owned(),
            ShellValue::Integer(i) => i.to_string(),
            ShellValue::AssociativeArray(_) => {
                todo!("UNIMPLEMENTED: converting associative array to string")
            }
            ShellValue::IndexedArray(values) => values.join(" "),
            ShellValue::Random => get_random_str(),
        }
    }
}

impl From<&str> for ShellValue {
    fn from(value: &str) -> Self {
        ShellValue::String(value.to_owned())
    }
}

impl From<&String> for ShellValue {
    fn from(value: &String) -> Self {
        ShellValue::String(value.clone())
    }
}

impl From<&[&str]> for ShellValue {
    fn from(values: &[&str]) -> Self {
        let owned_values: Vec<String> = values.iter().map(|v| (*v).to_string()).collect();
        ShellValue::IndexedArray(owned_values)
    }
}

impl From<&ShellValue> for String {
    fn from(value: &ShellValue) -> Self {
        match value {
            ShellValue::String(s) => s.clone(),
            ShellValue::Integer(i) => i.to_string(),
            ShellValue::AssociativeArray(_) => {
                todo!("UNIMPLEMENTED: converting associative array to string")
            }
            ShellValue::IndexedArray(values) => {
                values.first().map_or_else(String::new, |s| s.clone())
            }
            ShellValue::Random => get_random_str(),
        }
    }
}

fn get_random_str() -> String {
    let mut rng = rand::thread_rng();
    let value = rng.gen_range(0..32768);
    value.to_string()
}
