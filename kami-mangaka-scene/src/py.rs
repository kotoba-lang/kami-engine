// PyO3 bindings — exposed to lg_mangaka.compose_scene_3d.
// Built via `maturin build --features python` into the LangGraph pod image.
// P0 skeleton: surface only; methods delegate to the Rust core once render+sim land.

use pyo3::prelude::*;

use crate::{
    camera::CameraSpec,
    render::{RenderOpts, RenderResult},
    scene::{EnvironmentSpec, MangakaScene},
    SceneError,
};

#[pyclass(name = "MangakaScene")]
pub struct PyMangakaScene {
    inner: MangakaScene,
}

#[pymethods]
impl PyMangakaScene {
    #[new]
    fn new() -> Self {
        Self { inner: MangakaScene::new() }
    }

    fn set_background_json(&mut self, env_json: &str) -> PyResult<()> {
        let env: EnvironmentSpec = serde_json::from_str(env_json)
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyValueError, _>(e.to_string()))?;
        self.inner.set_background(env);
        Ok(())
    }

    fn set_camera_json(&mut self, cam_json: &str) -> PyResult<()> {
        let cam: CameraSpec = serde_json::from_str(cam_json)
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyValueError, _>(e.to_string()))?;
        self.inner.set_camera(cam);
        Ok(())
    }

    fn settle(&mut self, ticks: u32) {
        self.inner.settle(ticks);
    }

    fn to_jsonld(&self) -> PyResult<String> {
        Ok(self.inner.to_jsonld().to_string())
    }

    fn render_multi_json(&self, angles_json: &str, opts_json: &str) -> PyResult<Vec<Vec<u8>>> {
        let angles: Vec<CameraSpec> = serde_json::from_str(angles_json)
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyValueError, _>(e.to_string()))?;
        let opts: RenderOpts = serde_json::from_str(opts_json)
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyValueError, _>(e.to_string()))?;
        let results = self
            .inner
            .render_multi(&angles, opts)
            .map_err(|e: SceneError| {
                PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(e.to_string())
            })?;
        Ok(results.into_iter().map(|r: RenderResult| r.base_png).collect())
    }
}

#[pymodule]
fn kami_mangaka_scene(_py: Python<'_>, m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<PyMangakaScene>()?;
    Ok(())
}
