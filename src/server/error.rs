#[derive(Debug)]
pub(super) enum AppError {
    NotFound(String),
    BadRequest(String),
    Internal(String),
}
