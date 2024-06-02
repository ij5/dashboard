// use rustpython_vm::{self as vm};
// use vm::pymodule;

use rustpython_vm::pymodule;

#[pymodule]
pub mod dashboard_sys {
    use anyhow::Result;
    use crossbeam_channel::Sender;
    use futures::executor;
    use once_cell::sync::OnceCell;
    use rustpython_vm::{pyclass, PyPayload, PyResult, VirtualMachine};

    use crate::log;

    #[pyfunction]
    pub fn print(text: String) {
        let _ = log::println(&text);
    }

    #[derive(Debug)]
    pub struct Instance {
        sender: Sender<FrameData>
    }

    pub static INSTANCE: OnceCell<Instance> = OnceCell::new();

    pub fn initialize(sender: Sender<FrameData>) {
        INSTANCE.set(Instance {
            sender,
        }).expect("initialize failed");
    }

    #[pyattr]
    #[pyclass(module = "dashboard_sys", name = "_FrameData")]
    #[derive(Debug, PyPayload)]
    pub struct FrameData {
        action: String,
    }

    #[pyclass]
    impl FrameData {
        #[pymethod]
        fn display_data(&self) {
            log::println(&format!("{:?}", self)).expect("file print failed");
        }
    }

    #[pyfunction]
    pub fn fetch(method: String, url: String, vm: &VirtualMachine) -> PyResult<String> {
        executor::block_on(async {
            a_fetch(method, url)
                .await
                .map_err(|e| vm.new_runtime_error(e.to_string()))
        })
    }

    async fn a_fetch(method: String, url: String) -> Result<String> {
        let response;
        if method == "GET" {
            response = reqwest::get(url).await?.text().await?;
        } else {
            return Err(anyhow::Error::msg("method incorrect"));
        }
        Ok(response)
    }
}
