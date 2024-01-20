use anyhow::Result;
use rand::Rng;
use std::collections::HashMap;
use std::fmt::Write;

use crate::error;

#[derive(Clone, Debug)]
pub struct ShellVariable {
    pub value: ShellValue,
    pub exported: bool,
    pub readonly: bool,
    pub enumerable: bool,
    pub transform_on_update: ShellVariableUpdateTransform,
    pub trace: bool,
    pub treat_as_integer: bool,
}

#[derive(Clone, Debug)]
pub enum ShellVariableUpdateTransform {
    None,
    Lowercase,
    Uppercase,
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

    #[allow(clippy::unused_self)]
    pub fn set_by_str(&mut self, _value_str: &str) -> Result<(), error::Error> {
        error::unimp("set_by_str not implemented yet")
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

#[derive(Copy, Clone, Debug)]
pub enum FormatStyle {
    Basic,
    DeclarePrint,
}

impl ShellValue {
    #[allow(clippy::unnecessary_wraps)]
    pub fn new_indexed_array<S: AsRef<str>>(s: S) -> Result<Self, error::Error> {
        Ok(ShellValue::IndexedArray(vec![s.as_ref().to_owned()]))
    }

    pub fn new_associative_array<S: AsRef<str>>(_s: S) -> Result<Self, error::Error> {
        error::unimp("new associative array from string")
    }

    pub fn format(&self, style: FormatStyle) -> Result<String, error::Error> {
        match self {
            ShellValue::String(s) => {
                // TODO: Handle embedded newlines and other special chars.
                match style {
                    FormatStyle::Basic => {
                        if s.contains(' ') {
                            Ok(format!("'{s}'"))
                        } else {
                            Ok(s.clone())
                        }
                    }
                    FormatStyle::DeclarePrint => Ok(format!("\"{s}\"")),
                }
            }
            ShellValue::Integer(_) => error::unimp("formatting integers"),
            ShellValue::AssociativeArray(_) => error::unimp("formatting associative arrays"),
            ShellValue::IndexedArray(values) => {
                let mut result = String::new();
                result.push('(');

                for (i, value) in values.iter().enumerate() {
                    if i > 0 {
                        result.push(' ');
                    }
                    write!(result, "[{i}]=\"{value}\"")
                        .map_err(|e| error::Error::Unknown(e.into()))?;
                }

                result.push(')');
                Ok(result)
            }
            ShellValue::Random => error::unimp("formatting RANDOM"),
        }
    }

    pub fn get_at(&self, index: u32) -> Result<Option<&str>, error::Error> {
        match self {
            ShellValue::String(s) => {
                if index == 0 {
                    Ok(Some(s.as_str()))
                } else {
                    Ok(None)
                }
            }
            ShellValue::Integer(_) => error::unimp("indexing into integer"),
            ShellValue::AssociativeArray(_) => error::unimp("indexing into associative array"),
            ShellValue::IndexedArray(values) => Ok(values.get(index as usize).map(|s| s.as_str())),
            ShellValue::Random => error::unimp("indexing into RANDOM"),
        }
    }

    pub fn get_all(&self, _concatenate: bool) -> Result<String, error::Error> {
        // TODO: implement concatenate (or not)
        match self {
            ShellValue::String(s) => Ok(s.to_owned()),
            ShellValue::Integer(i) => Ok(i.to_string()),
            ShellValue::AssociativeArray(_) => {
                error::unimp("converting associative array to string")
            }
            ShellValue::IndexedArray(values) => Ok(values.join(" ")),
            ShellValue::Random => Ok(get_random_str()),
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
