use rand::Rng;
use std::borrow::Cow;
use std::collections::BTreeMap;
use std::fmt::{Display, Write};

use crate::{error, escape};

/// A shell variable.
#[derive(Clone, Debug)]
pub struct ShellVariable {
    /// The value currently associated with the variable.
    value: ShellValue,
    /// Whether or not the variable is marked as exported to child processes.
    exported: bool,
    /// Whether or not the variable is marked as read-only.
    readonly: bool,
    /// Whether or not the variable should be enumerated in the shell's environment.
    enumerable: bool,
    /// The transformation to apply to the variable's value when it is updated.
    transform_on_update: ShellVariableUpdateTransform,
    /// Whether or not the variable is marked as being traced.
    trace: bool,
    /// Whether or not the variable should be treated as an integer.
    treat_as_integer: bool,
    /// Whether or not the variable should be treated as a name reference.
    treat_as_nameref: bool,
}

/// Kind of transformation to apply to a variable's value when it is updated.
#[derive(Clone, Debug)]
pub enum ShellVariableUpdateTransform {
    /// No transformation.
    None,
    /// Convert the value to lowercase.
    Lowercase,
    /// Convert the value to uppercase.
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
    /// Returns a new shell variable, initialized with the given value.
    ///
    /// # Arguments
    ///
    /// * `value` - The value to associate with the variable.
    pub fn new(value: ShellValue) -> Self {
        Self {
            value,
            ..ShellVariable::default()
        }
    }

    /// Returns the value associated with the variable.
    pub fn value(&self) -> &ShellValue {
        &self.value
    }

    /// Returns whether or not the variable is exported to child processes.
    pub fn is_exported(&self) -> bool {
        self.exported
    }

    /// Marks the variable as exported to child processes.
    pub fn export(&mut self) {
        self.exported = true;
    }

    /// Marks the variable as not exported to child processes.
    pub fn unexport(&mut self) {
        self.exported = false;
    }

    /// Returns whether or not the variable is read-only.
    pub fn is_readonly(&self) -> bool {
        self.readonly
    }

    /// Marks the variable as read-only.
    pub fn set_readonly(&mut self) {
        self.readonly = true;
    }

    /// Marks the variable as not read-only.
    pub fn unset_readonly(&mut self) -> Result<(), error::Error> {
        if self.readonly {
            return Err(error::Error::ReadonlyVariable);
        }

        self.readonly = false;
        Ok(())
    }

    /// Returns whether or not the variable is traced.
    pub fn is_trace_enabled(&self) -> bool {
        self.trace
    }

    /// Marks the variable as traced.
    pub fn enable_trace(&mut self) {
        self.trace = true;
    }

    /// Marks the variable as not traced.
    pub fn disable_trace(&mut self) {
        self.trace = false;
    }

    /// Returns whether or not the variable should be enumerated in the shell's environment.
    pub fn is_enumerable(&self) -> bool {
        self.enumerable
    }

    /// Marks the variable as not enumerable in the shell's environment.
    pub fn hide_from_enumeration(&mut self) {
        self.enumerable = false;
    }

    /// Return the update transform associated with the variable.
    pub fn get_update_transform(&self) -> ShellVariableUpdateTransform {
        self.transform_on_update.clone()
    }

    /// Set the update transform associated with the variable.
    pub fn set_update_transform(&mut self, transform: ShellVariableUpdateTransform) {
        self.transform_on_update = transform;
    }

    /// Returns whether or not the variable should be treated as an integer.
    pub fn is_treated_as_integer(&self) -> bool {
        self.treat_as_integer
    }

    /// Marks the variable as being treated as an integer.
    pub fn treat_as_integer(&mut self) {
        self.treat_as_integer = true;
    }

    /// Marks the variable as not being treated as an integer.
    pub fn unset_treat_as_integer(&mut self) {
        self.treat_as_integer = false;
    }

    /// Returns whether or not the variable should be treated as a name reference.
    pub fn is_treated_as_nameref(&self) -> bool {
        self.treat_as_nameref
    }

    /// Marks the variable as being treated as a name reference.
    pub fn treat_as_nameref(&mut self) {
        self.treat_as_nameref = true;
    }

    /// Marks the variable as not being treated as a name reference.
    pub fn unset_treat_as_nameref(&mut self) {
        self.treat_as_nameref = false;
    }

    /// Converts the variable to an indexed array.
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

    /// Converts the variable to an associative array.
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

    /// Assign the given value to the variable, conditionally appending to the preexisting value.
    ///
    /// # Arguments
    ///
    /// * `value` - The value to assign to the variable.
    /// * `append` - Whether or not to append the value to the preexisting value.
    pub fn assign(&mut self, value: ShellValueLiteral, append: bool) -> Result<(), error::Error> {
        if self.is_readonly() {
            return Err(error::Error::ReadonlyVariable);
        }

        if append {
            match (&self.value, &value) {
                // If we're appending an array to a declared-but-unset variable (or appending
                // anything to a declared-but-unset array), then fill it out first.
                (ShellValue::Unset(_), ShellValueLiteral::Array(_))
                | (
                    ShellValue::Unset(
                        ShellValueUnsetType::IndexedArray | ShellValueUnsetType::AssociativeArray,
                    ),
                    _,
                ) => {
                    self.assign(ShellValueLiteral::Array(ArrayLiteral(vec![])), false)?;
                }
                // If we're trying to append an array to a string, we first promote the string to be
                // an array with the string being present at index 0.
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

                // If we're updating an indexed array value with an array, then preserve the array
                // type. We also default to using an indexed array if we are
                // assigning an array to a previously string-holding variable.
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

                // If we're updating an associative array value with an array, then preserve the
                // array type.
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

    /// Assign the given value to the variable at the given index, conditionally appending to the
    /// preexisting value present at that element within the value.
    ///
    /// # Arguments
    ///
    /// * `array_index` - The index at which to assign the value.
    /// * `value` - The value to assign to the variable at the given index.
    /// * `append` - Whether or not to append the value to the preexisting value stored at the given
    ///   index.
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
                tracing::error!("assigning to index {array_index} of {:?}", self.value);
                error::unimp("assigning to index of non-array variable")
            }
        }
    }

    /// Tries to unset the value stored at the given index in the variable. Returns
    /// whether or not a value was unset.
    ///
    /// # Arguments
    ///
    /// * `index` - The index at which to unset the value.
    pub fn unset_index(&mut self, index: &str) -> Result<bool, error::Error> {
        match &mut self.value {
            ShellValue::Unset(ty) => match ty {
                ShellValueUnsetType::Untyped => Err(error::Error::NotArray),
                ShellValueUnsetType::AssociativeArray | ShellValueUnsetType::IndexedArray => {
                    Ok(false)
                }
            },
            ShellValue::String(_) | ShellValue::Random => Err(error::Error::NotArray),
            ShellValue::AssociativeArray(values) => Ok(values.remove(index).is_some()),
            ShellValue::IndexedArray(values) => {
                let key = index.parse::<u64>().unwrap_or(0);
                Ok(values.remove(&key).is_some())
            }
        }
    }

    /// Returns the canonical attribute flag string for this variable.
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

        result
    }
}

/// A shell value.
#[derive(Clone, Debug)]
pub enum ShellValue {
    /// A value that has been typed but not yet set.
    Unset(ShellValueUnsetType),
    /// A string.
    String(String),
    /// An associative array.
    AssociativeArray(BTreeMap<String, String>),
    /// An indexed array.
    IndexedArray(BTreeMap<u64, String>),
    /// A special value that yields a different random number each time its read.
    Random,
}

/// The type of an unset shell value.
#[derive(Clone, Debug)]
pub enum ShellValueUnsetType {
    /// The value is untyped.
    Untyped,
    /// The value is an associative array.
    AssociativeArray,
    /// The value is an indexed array.
    IndexedArray,
}

/// A shell value literal; used for assignment.
#[derive(Clone, Debug)]
pub enum ShellValueLiteral {
    /// A scalar value.
    Scalar(String),
    /// An array value.
    Array(ArrayLiteral),
}

impl ShellValueLiteral {
    pub(crate) fn fmt_for_tracing(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ShellValueLiteral::Scalar(s) => Self::fmt_scalar_for_tracing(s.as_str(), f),
            ShellValueLiteral::Array(elements) => {
                write!(f, "(")?;
                for (i, (key, value)) in elements.0.iter().enumerate() {
                    if i > 0 {
                        write!(f, " ")?;
                    }
                    if let Some(key) = key {
                        write!(f, "[")?;
                        Self::fmt_scalar_for_tracing(key.as_str(), f)?;
                        write!(f, "]=")?;
                    }
                    Self::fmt_scalar_for_tracing(value.as_str(), f)?;
                }
                write!(f, ")")
            }
        }
    }

    fn fmt_scalar_for_tracing(s: &str, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let processed = escape::quote_if_needed(s, escape::QuoteMode::Quote);
        write!(f, "{processed}")
    }
}

impl Display for ShellValueLiteral {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.fmt_for_tracing(f)
    }
}

impl From<&str> for ShellValueLiteral {
    fn from(value: &str) -> Self {
        ShellValueLiteral::Scalar(value.to_owned())
    }
}

impl From<String> for ShellValueLiteral {
    fn from(value: String) -> Self {
        ShellValueLiteral::Scalar(value)
    }
}

impl From<Vec<&str>> for ShellValueLiteral {
    fn from(value: Vec<&str>) -> Self {
        ShellValueLiteral::Array(ArrayLiteral(
            value.into_iter().map(|s| (None, s.to_owned())).collect(),
        ))
    }
}

/// An array literal.
#[derive(Clone, Debug)]
pub struct ArrayLiteral(pub Vec<(Option<String>, String)>);

/// Style for formatting a shell variable's value.
#[derive(Copy, Clone, Debug)]
pub enum FormatStyle {
    /// Basic formatting.
    Basic,
    /// Formatting as appropriate in the `declare` built-in command.
    DeclarePrint,
}

impl ShellValue {
    /// Returns whether or not the value is an array.
    pub fn is_array(&self) -> bool {
        matches!(
            self,
            ShellValue::IndexedArray(_)
                | ShellValue::AssociativeArray(_)
                | ShellValue::Unset(
                    ShellValueUnsetType::IndexedArray | ShellValueUnsetType::AssociativeArray
                )
        )
    }

    /// Returns a new indexed array value constructed from the given slice of strings.
    ///
    /// # Arguments
    ///
    /// * `values` - The slice of strings to construct the indexed array from.
    pub fn indexed_array_from_slice(values: &[&str]) -> Self {
        let mut owned_values = BTreeMap::new();
        for (i, value) in values.iter().enumerate() {
            owned_values.insert(i as u64, (*value).to_string());
        }

        ShellValue::IndexedArray(owned_values)
    }

    /// Returns a new indexed array value constructed from the given literals.
    ///
    /// # Arguments
    ///
    /// * `literals` - The literals to construct the indexed array from.
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

    /// Returns a new associative array value constructed from the given literals.
    ///
    /// # Arguments
    ///
    /// * `literals` - The literals to construct the associative array from.
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

    /// Formats the value using the given style.
    ///
    /// # Arguments
    ///
    /// * `style` - The style to use for formatting the value.
    pub fn format(&self, style: FormatStyle) -> Result<Cow<'_, str>, error::Error> {
        match self {
            ShellValue::Unset(_) => Ok("".into()),
            ShellValue::String(s) => {
                // TODO: Handle embedded newlines and other special chars.
                match style {
                    FormatStyle::Basic => {
                        if s.contains(' ') {
                            Ok(format!("'{s}'").into())
                        } else {
                            Ok(s.into())
                        }
                    }
                    FormatStyle::DeclarePrint => Ok(format!("\"{s}\"").into()),
                }
            }
            ShellValue::AssociativeArray(values) => {
                let mut result = String::new();
                result.push('(');

                for (key, value) in values {
                    write!(result, "[{key}]=\"{value}\" ")?;
                }

                result.push(')');
                Ok(result.into())
            }
            ShellValue::IndexedArray(values) => {
                let mut result = String::new();
                result.push('(');

                for (i, (key, value)) in values.iter().enumerate() {
                    if i > 0 {
                        result.push(' ');
                    }
                    write!(result, "[{key}]=\"{value}\"")?;
                }

                result.push(')');
                Ok(result.into())
            }
            ShellValue::Random => Ok(std::format!("\"{}\"", get_random_str()).into()),
        }
    }

    /// Tries to retrieve the value stored at the given index in this variable.
    ///
    /// # Arguments
    ///
    /// * `index` - The index at which to retrieve the value.
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

    /// Returns the keys of the elements in this variable.
    pub fn get_element_keys(&self) -> Vec<String> {
        match self {
            ShellValue::Unset(_) => vec![],
            ShellValue::String(_) | ShellValue::Random => vec!["0".to_owned()],
            ShellValue::AssociativeArray(array) => array.keys().map(|k| k.to_owned()).collect(),
            ShellValue::IndexedArray(array) => array.keys().map(|k| k.to_string()).collect(),
        }
    }

    /// Returns the values of the elements in this variable.
    pub fn get_element_values(&self) -> Vec<String> {
        match self {
            ShellValue::Unset(_) => vec![],
            ShellValue::String(s) => vec![s.to_owned()],
            ShellValue::AssociativeArray(array) => array.values().map(|v| v.to_owned()).collect(),
            ShellValue::IndexedArray(array) => array.values().map(|v| v.to_owned()).collect(),
            ShellValue::Random => vec![get_random_str()],
        }
    }

    /// Converts this value to a string.
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

    /// Formats this value as a program string usable in an assignment.
    ///
    /// # Arguments
    ///
    /// * `index` - The index at which to retrieve the value, if indexing is to be performed.
    pub fn to_assignable_str(&self, index: Option<&str>) -> String {
        match self {
            ShellValue::Unset(_) => String::new(),
            ShellValue::String(s) => quote_str_for_assignment(s.as_str()),
            ShellValue::AssociativeArray(_) | ShellValue::IndexedArray(_) => {
                if let Some(index) = index {
                    if let Ok(Some(value)) = self.get_at(index) {
                        quote_str_for_assignment(value.as_ref())
                    } else {
                        String::new()
                    }
                } else {
                    self.format(FormatStyle::DeclarePrint).unwrap().into_owned()
                }
            }
            ShellValue::Random => quote_str_for_assignment(get_random_str().as_str()),
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

pub(crate) fn get_random_str() -> String {
    let mut rng = rand::thread_rng();
    let value = rng.gen_range(0..32768);
    value.to_string()
}

pub(crate) fn quote_str_for_assignment(s: &str) -> String {
    let mut result = String::new();

    let mut first = true;
    for part in s.split('\'') {
        if !first {
            result.push('\\');
            result.push('\'');
        } else {
            first = false;
        }

        if !part.is_empty() {
            result.push('\'');
            result.push_str(part);
            result.push('\'');
        }
    }

    result
}
