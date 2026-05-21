//! Input handling for map interactions (pan, zoom, tilt, rotate).

use crate::{KamiMap, ProjectionMode};
use kami_geo::projection;

static mut DRAGGING: bool = false;

pub fn on_pointer_down(map: &mut KamiMap, _x: f32, _y: f32, _button: u32) {
    unsafe {
        DRAGGING = true;
    }
    map.fly_target = None;
}

pub fn on_pointer_move(map: &mut KamiMap, dx: f32, dy: f32) {
    if unsafe { !DRAGGING } {
        return;
    }
    if map.projection_mode == ProjectionMode::Globe || map.projection_mode == ProjectionMode::Cosmic {
        let pixels_per_degree = (map.width.max(map.height) as f64 * 0.85).max(240.0);
        map.center.lng -= dx as f64 * 180.0 / pixels_per_degree;
        map.center.lat += dy as f64 * 120.0 / pixels_per_degree;
        map.center.lat = projection::clamp_lat(map.center.lat);
        if map.center.lng > 180.0 {
            map.center.lng -= 360.0;
        } else if map.center.lng < -180.0 {
            map.center.lng += 360.0;
        }
        return;
    }
    let center_px = projection::lng_lat_to_world_px(map.center, map.zoom);
    let new_px = projection::WorldPx {
        x: center_px.x - dx as f64,
        y: center_px.y - dy as f64,
    };
    map.center = projection::world_px_to_lng_lat(new_px, map.zoom);
}

pub fn on_pointer_up(_map: &mut KamiMap) {
    unsafe {
        DRAGGING = false;
    }
}
