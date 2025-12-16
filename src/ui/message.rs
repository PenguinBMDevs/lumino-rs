
use super::router::Route;

#[derive(Debug, Clone)]
pub enum Message {
    RouteUpdated(Route),
}
