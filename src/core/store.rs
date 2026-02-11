use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StoreKind {
    User,
    Repo,
}

#[derive(Debug, Clone)]
pub struct Store {
    pub kind: StoreKind,
    pub root: PathBuf,
}
