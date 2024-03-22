use anyhow::Result;
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

    fn convert_to_indexed_array(&mut self) {
        let mut new_values = BTreeMap::new();
        new_values.insert(0, String::from(&self.value));
        self.value = ShellValue::IndexedArray(new_values);
    }

    pub fn assign(&mut self, value: ShellValueLiteral, append: bool) -> Result<(), error::Error> {
        if self.is_readonly() {
            return Err(error::Error::ReadonlyVariable);
        }

        if append {
            // If we're trying to append an array to a string, we first promote the string to be an array
            // with the string being present at index 0.
            if matches!(self.value, ShellValue::String(_))
                && matches!(value, ShellValueLiteral::Array(_))
            {
                self.convert_to_indexed_array();
            }

            let treat_as_int = self.is_treated_as_integer();

            match &mut self.value {
                ShellValue::String(base) => match value {
                    ShellValueLiteral::Scalar(suffix) => {
                        if treat_as_int {
                            let int_value = base.parse::<i64>().unwrap_or(0)
                                + suffix.parse::<i64>().unwrap_or(0);
                            base.clear();
                            base.push_str(int_value.to_string().as_str());
                        } else {
                            base.push_str(suffix.as_str());
                        }
                        Ok(())
                    }
                    ShellValueLiteral::Array(_) => {
                        // This case was already handled (see above).
                        Ok(())
                    }
                },
                ShellValue::IndexedArray(existing_values) => match value {
                    ShellValueLiteral::Scalar(new_value) => {
                        self.assign_at_index("0", new_value.as_str(), append)
                    }
                    ShellValueLiteral::Array(new_values) => {
                        let mut new_key =
                            if let Some((largest_index, _)) = existing_values.last_key_value() {
                                largest_index + 1
                            } else {
                                0
                            };

                        for (key, value) in new_values.0 {
                            if let Some(key) = key {
                                new_key = key.parse().unwrap_or(0);
                            }

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
            match (&self.value, value) {
                // If we're updating an array value with a string, then treat it as an update to
                // just the "0"-indexed element of the array.
                (
                    ShellValue::IndexedArray(_)
                    | ShellValue::AssociativeArray(_)
                    | ShellValue::Unset(
                        ShellValueUnsetType::AssociativeArray | ShellValueUnsetType::IndexedArray,
                    ),
                    ShellValueLiteral::Scalar(s),
                ) => self.assign_at_index("0", s.as_str(), false),

                // If we're updating an indexed array value with an array, then preserve the array type.
                // We also default to using an indexed array if we are assigning an array to a previously
                // string-holding variable.
                (
                    ShellValue::IndexedArray(_)
                    | ShellValue::Unset(
                        ShellValueUnsetType::IndexedArray | ShellValueUnsetType::Untyped,
                    )
                    | ShellValue::String(_)
                    | ShellValue::Random,
                    ShellValueLiteral::Array(values),
                ) => {
                    self.value = ShellValue::indexed_array_from_literals(values);
                    Ok(())
                }

                // If we're updating an associative array value with an array, then preserve the array type.
                (
                    ShellValue::AssociativeArray(_)
                    | ShellValue::Unset(ShellValueUnsetType::AssociativeArray),
                    ShellValueLiteral::Array(values),
                ) => {
                    self.value = ShellValue::associative_array_from_literals(values);
                    Ok(())
                }

                // Drop other updates to random values.
                (ShellValue::Random, _) => Ok(()),

                // Assign a scalar value to a scalar or unset (and untyped) variable.
                (ShellValue::String(_) | ShellValue::Unset(_), ShellValueLiteral::Scalar(s)) => {
                    self.value = ShellValue::String(s);
                    Ok(())
                }
            }
        }
    }

    #[allow(clippy::unused_self)]
    #[allow(clippy::needless_pass_by_value)]
    pub fn assign_at_index(
        &mut self,
        array_index: &str,
        value: &str,
        append: bool,
    ) -> Result<(), error::Error> {
        match &self.value {
            ShellValue::Unset(_) => {
                self.assign(ShellValueLiteral::Array(ArrayLiteral(vec![])), false)?;
            }
            ShellValue::String(_) => {
                self.convert_to_indexed_array();
            }
            _ => (),
        }

        let treat_as_int = self.is_treated_as_integer();

        match &mut self.value {
            ShellValue::IndexedArray(arr) => {
                let key: u64 = array_index.parse().unwrap_or(0);

                if append {
                    let existing_value = arr.get(&key);

                    if treat_as_int {
                        return error::unimp("append-assignment to int element of indexed array");
                    } else {
                        let mut new_value = existing_value.map_or_else(String::new, |v| v.clone());
                        new_value.push_str(value);
                        arr.insert(key, new_value);
                    }
                } else {
                    arr.insert(key, value.to_owned());
                }

                Ok(())
            }
            ShellValue::AssociativeArray(arr) => {
                if append {
                    return error::unimp("append-assignment to index of associative array");
                } else {
                    arr.insert(array_index.to_owned(), value.to_owned());
                }
                Ok(())
            }
            _ => {
                log::error!("assigning to index {array_index} of {:?}", self.value);
                error::unimp("assigning to index of non-array variable")
            }
        }
    }
}

#[derive(Clone, Debug)]
pub enum ShellValue {
    Unset(ShellValueUnsetType),
    String(String),
    AssociativeArray(BTreeMap<String, String>),
    IndexedArray(BTreeMap<u64, String>),
    Random,
}

#[derive(Clone, Debug)]
pub enum ShellValueUnsetType {
    Untyped,
    AssociativeArray,
    IndexedArray,
}

#[derive(Clone, Debug)]
pub enum ShellValueLiteral {
    Scalar(String),
    Array(ArrayLiteral),
}

#[derive(Clone, Debug)]
pub struct ArrayLiteral(pub Vec<(Option<String>, String)>);

#[derive(Copy, Clone, Debug)]
pub enum FormatStyle {
    Basic,
    DeclarePrint,
}

impl ShellValue {
    pub fn indexed_array_from_slice(values: &[&str]) -> Self {
        let mut owned_values = BTreeMap::new();
        for (i, value) in values.iter().enumerate() {
            owned_values.insert(i as u64, (*value).to_string());
        }

        ShellValue::IndexedArray(owned_values)
    }

    pub fn indexed_array_from_literals(values: ArrayLiteral) -> Self {
        let mut arr = BTreeMap::new();

        let mut key: u64 = 0;
        for (literal_key, value) in values.0 {
            if let Some(literal_key) = literal_key {
                key = literal_key.parse().unwrap_or(0);
            }

            arr.insert(key, value);
            key += 1;
        }

        ShellValue::IndexedArray(arr)
    }

    pub fn associative_array_from_literals(values: ArrayLiteral) -> Self {
        let mut arr = BTreeMap::new();

        let mut current_key = None;
        for (literal_key, value) in values.0 {
            if let Some(literal_key) = literal_key {
                current_key = Some(literal_key);
            }

            if let Some(key) = current_key {
                current_key = None;
                arr.insert(key, value);
            } else {
                current_key = Some(value);
            }
        }

        if let Some(key) = current_key {
            arr.insert(key, String::new());
        }

        ShellValue::AssociativeArray(arr)
    }

    pub fn format(&self, style: FormatStyle) -> Result<String, error::Error> {
        match self {
            ShellValue::Unset(_) => Ok(String::new()),
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
            ShellValue::AssociativeArray(values) => {
                let mut result = String::new();
                result.push('(');

                for (key, value) in values {
                    write!(result, "[{key}]=\"{value}\" ")
                        .map_err(|e| error::Error::Unknown(e.into()))?;
                }

                result.push(')');
                Ok(result)
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

    #[allow(clippy::unnecessary_wraps)]
    pub fn get_at(&self, index: &str) -> Result<Option<String>, error::Error> {
        match self {
            ShellValue::Unset(_) => Ok(None),
            ShellValue::String(s) => {
                if index.parse::<u64>().unwrap_or(0) == 0 {
                    Ok(Some(s.to_owned()))
                } else {
                    Ok(None)
                }
            }
            ShellValue::AssociativeArray(values) => Ok(values.get(index).map(|s| s.to_owned())),
            ShellValue::IndexedArray(values) => {
                let key = index.parse::<u64>().unwrap_or(0);
                Ok(values.get(&key).map(|s| s.to_owned()))
            }
            ShellValue::Random => Ok(Some(get_random_str())),
        }
    }

    #[allow(clippy::unnecessary_wraps)]
    pub fn get_all_elements(&self) -> Result<Vec<String>, error::Error> {
        let result = match self {
            ShellValue::Unset(_) => vec![],
            ShellValue::String(s) => vec![s.to_owned()],
            ShellValue::AssociativeArray(arr) => arr.values().map(|s| s.to_owned()).collect(),
            ShellValue::IndexedArray(arr) => arr.values().map(|s| s.to_owned()).collect(),
            ShellValue::Random => vec![get_random_str()],
        };

        Ok(result)
    }

    #[allow(clippy::unnecessary_wraps)]
    pub fn get_all(&self) -> Result<String, error::Error> {
        match self {
            ShellValue::Unset(_) => Ok(String::new()),
            ShellValue::String(s) => Ok(s.to_owned()),
            ShellValue::AssociativeArray(values) => {
                let mut formatted = String::new();

                for (i, (_key, value)) in values.iter().enumerate() {
                    if i > 0 {
                        formatted.push(' ');
                    }
                    formatted.push_str(value);
                }

                Ok(formatted)
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

impl From<&ShellValue> for String {
    fn from(value: &ShellValue) -> Self {
        match value {
            ShellValue::Unset(_) => String::new(),
            ShellValue::String(s) => s.clone(),
            ShellValue::AssociativeArray(values) => {
                values.get("0").map_or_else(String::new, |s| s.clone())
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
