use std::{collections::HashMap, sync::Arc};

#[derive(Clone, Default)]
pub struct FunctionEnv {
    functions: HashMap<String, FunctionRegistration>,
}

impl FunctionEnv {
    pub fn get(&self, name: &str) -> Option<&FunctionRegistration> {
        self.functions.get(name)
    }

    pub fn remove(&mut self, name: &str) -> Option<FunctionRegistration> {
        self.functions.remove(name)
    }

    pub fn update(&mut self, name: String, definition: Arc<parser::ast::FunctionDefinition>) {
        self.functions
            .insert(name, FunctionRegistration { definition });
    }

    pub fn iter(&self) -> impl Iterator<Item = (&String, &FunctionRegistration)> {
        self.functions.iter()
    }
}

#[derive(Clone)]
pub struct FunctionRegistration {
    pub definition: Arc<parser::ast::FunctionDefinition>,
}
