use netroute::RouteEntry;
use std::io;

pub fn list_routes() -> io::Result<Vec<RouteEntry>> {
    netroute::list_routes()
}
