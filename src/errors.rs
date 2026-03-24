#[derive(Debug)]
pub enum InsertError {
    Exists,
    Full,
    Race,
    StateBreached,
}

#[derive(Debug)]
pub enum DeleteResult {
    Deleted,
    NotFound,
    Poisoned,
}

#[derive(Debug)]
pub enum UpdateResult {
    Poisoned,
    Updated,
    UpdateFailed,
    NotFound,
}

pub enum CreateError {
    InvalidSize,
}
