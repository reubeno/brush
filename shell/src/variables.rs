use anyhow::Result;
use rand::Rng;
use std::borrow::Cow;
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
    treat_as_nameref: bool,
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
            treat_as_nameref: false,
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

    pub fn unset_readonly(&mut self) -> Result<(), error::Error> {
        if self.readonly {
            return Err(error::Error::ReadonlyVariable);
        }

        self.readonly = false;
        Ok(())
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

    pub fn is_treated_as_nameref(&self) -> bool {
        self.treat_as_nameref
    }

    pub fn treat_as_nameref(&mut self) {
        self.treat_as_nameref = true;
    }

    pub fn unset_treat_as_nameref(&mut self) {
        self.treat_as_nameref = false;
    }

    pub fn convert_to_indexed_array(&mut self) -> Result<(), error::Error> {
        match self.value() {
            ShellValue::IndexedArray(_) => Ok(()),
            ShellValue::AssociativeArray(_) => {
                Err(error::Error::ConvertingAssociativeArrayToIndexedArray)
            }
            _ => {
                let mut new_values = BTreeMap::new();
                new_values.insert(0, self.value.to_cow_string().to_string());
                self.value = ShellValue::IndexedArray(new_values);
                Ok(())
            }
        }
    }

    pub fn convert_to_associative_array(&mut self) -> Result<(), error::Error> {
        match self.value() {
            ShellValue::AssociativeArray(_) => Ok(()),
            ShellValue::IndexedArray(_) => {
                Err(error::Error::ConvertingIndexedArrayToAssociativeArray)
            }
            _ => {
                let mut new_values: BTreeMap<String, String> = BTreeMap::new();
                new_values.insert(String::from("0"), self.value.to_cow_string().to_string());
                self.value = ShellValue::AssociativeArray(new_values);
                Ok(())
            }
        }
    }

    pub fn assign(&mut self, value: ShellValueLiteral, append: bool) -> Result<(), error::Error> {
        if self.is_readonly() {
            return Err(error::Error::ReadonlyVariable);
        }

        if append {
            match (&self.value, &value) {
                // If we're appending an array to a declared-but-unset variable (or appending anything to a declared-but-unset array),
                // then fill it out first.
                (ShellValue::Unset(_), ShellValueLiteral::Array(_))
                | (
                    ShellValue::Unset(
                        ShellValueUnsetType::IndexedArray | ShellValueUnsetType::AssociativeArray,
                    ),
                    _,
                ) => {
                    self.assign(ShellValueLiteral::Array(ArrayLiteral(vec![])), false)?;
                }
                // If we're trying to append an array to a string, we first promote the string to be an array
                // with the string being present at index 0.
                (ShellValue::String(_), ShellValueLiteral::Array(_)) => {
                    self.convert_to_indexed_array()?;
                }
                _ => (),
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
                        self.assign_at_index(String::from("0"), new_value, append)
                    }
                    ShellValueLiteral::Array(new_values) => {
                        ShellValue::update_indexed_array_from_literals(existing_values, new_values)
                    }
                },
                ShellValue::AssociativeArray(existing_values) => match value {
                    ShellValueLiteral::Scalar(new_value) => {
                        self.assign_at_index(String::from("0"), new_value, append)
                    }
                    ShellValueLiteral::Array(new_values) => {
                        ShellValue::update_associative_array_from_literals(
                            existing_values,
                            new_values,
                        )
                    }
                },
                ShellValue::Unset(_) => error::unimp("appending to unset variable"),
                ShellValue::Random => Ok(()),
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
                ) => self.assign_at_index(String::from("0"), s, false),

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
                    ShellValueLiteral::Array(literal_values),
                ) => {
                    self.value = ShellValue::indexed_array_from_literals(literal_values)?;
                    Ok(())
                }

                // If we're updating an associative array value with an array, then preserve the array type.
                (
                    ShellValue::AssociativeArray(_)
                    | ShellValue::Unset(ShellValueUnsetType::AssociativeArray),
                    ShellValueLiteral::Array(literal_values),
                ) => {
                    self.value = ShellValue::associative_array_from_literals(literal_values)?;
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
        array_index: String,
        value: String,
        append: bool,
    ) -> Result<(), error::Error> {
        match &self.value {
            ShellValue::Unset(_) => {
                self.assign(ShellValueLiteral::Array(ArrayLiteral(vec![])), false)?;
            }
            ShellValue::String(_) => {
                self.convert_to_indexed_array()?;
            }
            _ => (),
        }

        let treat_as_int = self.is_treated_as_integer();

        match &mut self.value {
            ShellValue::IndexedArray(arr) => {
                let key: u64 = array_index.parse().unwrap_or(0);

                if append {
                    let existing_value = arr.get(&key).map_or_else(|| "", |v| v.as_str());

                    let mut new_value;
                    if treat_as_int {
                        new_value = (existing_value.parse::<i64>().unwrap_or(0)
                            + value.parse::<i64>().unwrap_or(0))
                        .to_string();
                    } else {
                        new_value = existing_value.to_owned();
                        new_value.push_str(value.as_str());
                    }

                    arr.insert(key, new_value);
                } else {
                    arr.insert(key, value);
                }

                Ok(())
            }
            ShellValue::AssociativeArray(arr) => {
                if append {
                    let existing_value = arr
                        .get(array_index.as_str())
                        .map_or_else(|| "", |v| v.as_str());

                    let mut new_value;
                    if treat_as_int {
                        new_value = (existing_value.parse::<i64>().unwrap_or(0)
                            + value.parse::<i64>().unwrap_or(0))
                        .to_string();
                    } else {
                        new_value = existing_value.to_owned();
                        new_value.push_str(value.as_str());
                    }

                    arr.insert(array_index, new_value.to_string());
                } else {
                    arr.insert(array_index, value);
                }
                Ok(())
            }
            _ => {
                log::error!("assigning to index {array_index} of {:?}", self.value);
                error::unimp("assigning to index of non-array variable")
            }
        }
    }

    pub fn get_attribute_flags(&self) -> String {
        let mut result = String::new();

        if matches!(
            self.value(),
            ShellValue::IndexedArray(_) | ShellValue::Unset(ShellValueUnsetType::IndexedArray)
        ) {
            result.push('a');
        }
        if matches!(
            self.value(),
            ShellValue::AssociativeArray(_)
                | ShellValue::Unset(ShellValueUnsetType::AssociativeArray)
        ) {
            result.push('A');
        }
        if self.is_treated_as_integer() {
            result.push('i');
        }
        if self.is_treated_as_nameref() {
            result.push('n');
        }
        if self.is_readonly() {
            result.push('r');
        }
        if let ShellVariableUpdateTransform::Lowercase = self.get_update_transform() {
            result.push('l');
        }
        if self.is_trace_enabled() {
            result.push('t');
        }
        if let ShellVariableUpdateTransform::Uppercase = self.get_update_transform() {
            result.push('u');
        }
        if self.is_exported() {
            result.push('x');
        }

        if result.is_empty() {
            result.push('-');
        }

        result
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

    pub fn indexed_array_from_literals(literals: ArrayLiteral) -> Result<ShellValue, error::Error> {
        let mut values = BTreeMap::new();
        ShellValue::update_indexed_array_from_literals(&mut values, literals)?;

        Ok(ShellValue::IndexedArray(values))
    }

    #[allow(clippy::unnecessary_wraps)]
    fn update_indexed_array_from_literals(
        existing_values: &mut BTreeMap<u64, String>,
        literal_values: ArrayLiteral,
    ) -> Result<(), error::Error> {
        let mut new_key = if let Some((largest_index, _)) = existing_values.last_key_value() {
            largest_index + 1
        } else {
            0
        };

        for (key, value) in literal_values.0 {
            if let Some(key) = key {
                new_key = key.parse().unwrap_or(0);
            }

            existing_values.insert(new_key, value);
            new_key += 1;
        }

        Ok(())
    }

    pub fn associative_array_from_literals(
        literals: ArrayLiteral,
    ) -> Result<ShellValue, error::Error> {
        let mut values = BTreeMap::new();
        ShellValue::update_associative_array_from_literals(&mut values, literals)?;

        Ok(ShellValue::AssociativeArray(values))
    }

    fn update_associative_array_from_literals(
        existing_values: &mut BTreeMap<String, String>,
        literal_values: ArrayLiteral,
    ) -> Result<(), error::Error> {
        let mut current_key = None;
        for (key, value) in literal_values.0 {
            if let Some(current_key) = current_key.take() {
                if key.is_some() {
                    return error::unimp("misaligned keys/values in associative array literal");
                } else {
                    existing_values.insert(current_key, value);
                }
            } else if let Some(key) = key {
                existing_values.insert(key, value);
            } else {
                current_key = Some(value);
            }
        }

        if let Some(current_key) = current_key {
            existing_values.insert(current_key, String::new());
        }

        Ok(())
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
    pub fn get_at(&self, index: &str) -> Result<Option<Cow<'_, str>>, error::Error> {
        match self {
            ShellValue::Unset(_) => Ok(None),
            ShellValue::String(s) => {
                if index.parse::<u64>().unwrap_or(0) == 0 {
                    Ok(Some(Cow::Borrowed(s)))
                } else {
                    Ok(None)
                }
            }
            ShellValue::AssociativeArray(values) => {
                Ok(values.get(index).map(|s| Cow::Borrowed(s.as_str())))
            }
            ShellValue::IndexedArray(values) => {
                let key = index.parse::<u64>().unwrap_or(0);
                Ok(values.get(&key).map(|s| Cow::Borrowed(s.as_str())))
            }
            ShellValue::Random => Ok(Some(Cow::Owned(get_random_str()))),
        }
    }

    pub fn get_element_keys(&self) -> Vec<String> {
        match self {
            ShellValue::Unset(_) => vec![],
            ShellValue::String(_) | ShellValue::Random => vec!["0".to_owned()],
            ShellValue::AssociativeArray(array) => array.keys().map(|k| k.to_owned()).collect(),
            ShellValue::IndexedArray(array) => array.keys().map(|k| k.to_string()).collect(),
        }
    }

    pub fn get_element_values(&self) -> Vec<String> {
        match self {
            ShellValue::Unset(_) => vec![],
            ShellValue::String(s) => vec![s.to_owned()],
            ShellValue::AssociativeArray(array) => array.values().map(|v| v.to_owned()).collect(),
            ShellValue::IndexedArray(array) => array.values().map(|v| v.to_owned()).collect(),
            ShellValue::Random => vec![get_random_str()],
        }
    }

    pub fn to_cow_string(&self) -> Cow<'_, str> {
        match self {
            ShellValue::Unset(_) => Cow::Borrowed(""),
            ShellValue::String(s) => Cow::Borrowed(s.as_str()),
            ShellValue::AssociativeArray(values) => values
                .get("0")
                .map_or_else(|| Cow::Borrowed(""), |s| Cow::Borrowed(s.as_str())),
            ShellValue::IndexedArray(values) => values
                .get(&0)
                .map_or_else(|| Cow::Borrowed(""), |s| Cow::Borrowed(s.as_str())),
            ShellValue::Random => Cow::Owned(get_random_str()),
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

fn get_random_str() -> String {
    let mut rng = rand::thread_rng();
    let value = rng.gen_range(0..32768);
    value.to_string()
}
