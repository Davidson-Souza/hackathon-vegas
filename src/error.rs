use axum::response::IntoResponse;

#[derive(Debug)]
pub enum Error {
    NotFound,
    BadRequest,
    DbError,
    Hasher,
    Server,
}

impl From<std::io::Error> for Error {
    fn from(_: std::io::Error) -> Self {
        Error::Hasher
    }
}

impl From<sqlite::Error> for Error {
    fn from(_: sqlite::Error) -> Self {
        Error::DbError
    }
}

impl IntoResponse for Error {
    fn into_response(self) -> axum::http::Response<axum::body::Body> {
        match self {
            Error::NotFound => axum::http::Response::builder()
                .status(404)
                .body(axum::body::Body::from("Not Found"))
                .unwrap(),
            Error::BadRequest => axum::http::Response::builder()
                .status(400)
                .body(axum::body::Body::from("Bad Request"))
                .unwrap(),
            Error::DbError => axum::http::Response::builder()
                .status(500)
                .body(axum::body::Body::from("Database Error"))
                .unwrap(),
            Error::Hasher => axum::http::Response::builder()
                .status(500)
                .body(axum::body::Body::from("Hasher Error"))
                .unwrap(),
            Error::Server => axum::http::Response::builder()
                .status(500)
                .body(axum::body::Body::from("Server Error"))
                .unwrap(),
        }
    }
}
