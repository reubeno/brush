use anyhow::Result;
use itertools::Itertools;
use rand::Rng;
use std::collections::BTreeMap;
use std::fmt::Write;

use crate::error;

#[derive(Clone, Debug)]
pub struct ShellVariable {
    value: ShellValue,
    exported: bool,
    readonly: bool,
    enumerable: bool,
    transform_on_update: ShellVariableUpdateTransform,
    trace: bool,
    treat_as_integer: bool,
}

#[derive(Clone, Debug)]
pub enum ShellVariableUpdateTransform {
    None,
    Lowercase,
    Uppercase,
}

impl Default for ShellVariable {
    fn default() -> Self {
        Self {
            value: ShellValue::String(String::new()),
            exported: false,
            readonly: false,
            enumerable: true,
            transform_on_update: ShellVariableUpdateTransform::None,
            trace: false,
            treat_as_integer: false,
        }
    }
}

impl ShellVariable {
    pub fn new(value: ShellValue) -> Self {
        Self {
            value,
            ..ShellVariable::default()
        }
    }

    pub fn value(&self) -> &ShellValue {
        &self.value
    }

    pub fn is_exported(&self) -> bool {
        self.exported
    }

    pub fn export(&mut self) {
        self.exported = true;
    }

    pub fn unexport(&mut self) {
        self.exported = false;
    }

    pub fn is_readonly(&self) -> bool {
        self.readonly
    }

    pub fn set_readonly(&mut self) {
        self.readonly = true;
    }

    pub fn unset_readonly(&mut self) {
        self.readonly = false;
    }

    pub fn is_trace_enabled(&self) -> bool {
        self.trace
    }

    pub fn enable_trace(&mut self) {
        self.trace = true;
    }

    pub fn disable_trace(&mut self) {
        self.trace = false;
    }

    pub fn is_enumerable(&self) -> bool {
        self.enumerable
    }

    pub fn hide_from_enumeration(&mut self) {
        self.enumerable = false;
    }

    pub fn get_update_transform(&self) -> ShellVariableUpdateTransform {
        self.transform_on_update.clone()
    }

    pub fn set_update_transform(&mut self, transform: ShellVariableUpdateTransform) {
        self.transform_on_update = transform;
    }

    pub fn is_treated_as_integer(&self) -> bool {
        self.treat_as_integer
    }

    pub fn treat_as_integer(&mut self) {
        self.treat_as_integer = true;
    }

    pub fn unset_treat_as_integer(&mut self) {
        self.treat_as_integer = false;
    }

    #[allow(clippy::unused_self)]
    pub fn set_by_str(&mut self, _value_str: &str) -> Result<(), error::Error> {
        error::unimp("set_by_str not implemented yet")
    }

    pub fn assign(&mut self, value: ScalarOrArray, append: bool) -> Result<(), error::Error> {
        if append {
            // If we're trying to append an array to a string, we first promote the string to be an array
            // with the string being present at index 0.
            if matches!(self.value, ShellValue::String(_))
                && matches!(value, ScalarOrArray::Array(_))
            {
                let mut new_values = BTreeMap::new();
                new_values.insert(0, String::from(&self.value));
                self.value = ShellValue::IndexedArray(new_values);
            }

            match &mut self.value {
                ShellValue::String(base) => match value {
                    ScalarOrArray::Scalar(suffix) => {
                        base.push_str(suffix.as_str());
                        Ok(())
                    }
                    ScalarOrArray::Array(_) => {
                        // This case was already handled (see above).
                        Ok(())
                    }
                },
                ShellValue::IndexedArray(existing_values) => match value {
                    ScalarOrArray::Scalar(_) => error::unimp("appending scalar to array"),
                    ScalarOrArray::Array(new_values) => {
                        let mut new_key =
                            if let Some((largest_index, _)) = existing_values.last_key_value() {
                                largest_index + 1
                            } else {
                                0
                            };

                        for (_key, value) in new_values {
                            // TODO: do something with the key!
                            existing_values.insert(new_key, value);
                            new_key += 1;
                        }

                        Ok(())
                    }
                },
                _ => error::unimp("appending to unsupported variable type"),
            }
        } else {
            // If we're updating an array value with a string, then treat it as an update to
            // just the 0th item of the array.
            if matches!(self.value, ShellValue::IndexedArray(_))
                && matches!(value, ScalarOrArray::Scalar(_))
            {
                self.assign_at_index("0", value, false)?;
            } else {
                self.value = value.into();
            }

            Ok(())
        }
    }

    #[allow(clippy::unused_self)]
    #[allow(clippy::needless_pass_by_value)]
    pub fn assign_at_index(
        &mut self,
        array_index: &str,
        value: ScalarOrArray,
        append: bool,
    ) -> Result<(), error::Error> {
        if append {
            return error::unimp("appending during assignment through index");
        }

        match &mut self.value {
            ShellValue::IndexedArray(arr) => {
                match value {
                    ScalarOrArray::Scalar(s) => {
                        let key: u64 = array_index.parse().unwrap_or(0);
                        arr.insert(key, s);
                    }
                    ScalarOrArray::Array(_) => {
                        return error::unimp("assigning array to array index");
                    }
                }
                Ok(())
            }
            ShellValue::AssociativeArray(arr) => {
                arr.insert(array_index.to_owned(), value.into());
                Ok(())
            }
            _ => error::unimp("assigning to index of non-array variable"),
        }
    }
}

#[derive(Clone, Debug)]
pub enum ShellValue {
    String(String),
    Integer(u64),
    AssociativeArray(BTreeMap<String, ShellValue>),
    IndexedArray(BTreeMap<u64, String>),
    Random,
}

#[derive(Clone, Debug)]
pub enum ScalarOrArray {
    Scalar(String),
    Array(Vec<(Option<String>, String)>),
}

#[derive(Copy, Clone, Debug)]
pub enum FormatStyle {
    Basic,
    DeclarePrint,
}

impl ShellValue {
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
            ShellValue::AssociativeArray(values) => {
                let arr_str = values
                    .iter()
                    .map(|(k, v)| format!("[{}]={}", k, String::from(v)))
                    .join(" ");

                Ok(arr_str)
            }
            ShellValue::IndexedArray(values) => {
                let mut result = String::new();
                result.push('(');

                for (i, (key, value)) in values.iter().enumerate() {
                    if i > 0 {
                        result.push(' ');
                    }
                    write!(result, "[{key}]=\"{value}\"")
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
            ShellValue::IndexedArray(values) => {
                Ok(values.get(&(u64::from(index))).map(|s| s.as_str()))
            }
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
            ShellValue::IndexedArray(values) => {
                let mut formatted = String::new();

                for (i, (_key, value)) in values.iter().enumerate() {
                    if i > 0 {
                        formatted.push(' ');
                    }
                    formatted.push_str(value);
                }

                Ok(formatted)
            }
            ShellValue::Random => Ok(get_random_str()),
        }
    }
}

impl From<ScalarOrArray> for ShellValue {
    fn from(value: ScalarOrArray) -> Self {
        match value {
            ScalarOrArray::Scalar(value) => ShellValue::String(value),
            ScalarOrArray::Array(values) => {
                let mut converted = BTreeMap::new();

                // TODO: do something with key
                for (i, (_key, value)) in values.iter().enumerate() {
                    converted.insert(i as u64, value.to_owned());
                }

                ShellValue::IndexedArray(converted)
            }
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
        let mut owned_values = BTreeMap::new();
        for (i, value) in values.iter().enumerate() {
            owned_values.insert(i as u64, (*value).to_string());
        }

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
                values.get(&0).map_or_else(String::new, |s| s.clone())
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
