/// API data transfer objects (DTOs) for the Crystal Forge REST API.
///
/// These types define the JSON contracts between the server and UI clients.
/// They are deliberately decoupled from the database models in `crate::models`
/// to allow the API contract to evolve independently of schema changes.
pub mod models;
