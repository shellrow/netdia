use std::{collections::HashMap, net::IpAddr};

use anyhow::Result;
use netdev::MacAddr;
use tauri::State;

use crate::{net::route::list_routes, state::SharedState};
use netroute::RouteEntry;

#[tauri::command]
pub async fn get_routes(_state: State<'_, SharedState>) -> Result<Vec<RouteEntry>, String> {
    list_routes().map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_neighbor_table(
    _state: State<'_, SharedState>,
) -> Result<HashMap<IpAddr, MacAddr>, String> {
    crate::net::neigh::get_neighbor_table().map_err(|e| e.to_string())
}
