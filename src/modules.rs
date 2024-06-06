// use rustpython_vm::{self as vm};
// use vm::pymodule;

use rustpython_vm::pymodule;

#[pymodule]
pub mod dashboard_sys {
    use color_eyre::eyre::Result;
    use crossbeam_channel::Sender;
    use futures::executor;
    use once_cell::sync::OnceCell;
    use rustpython_vm::{PyObject, PyResult, TryFromBorrowedObject, VirtualMachine};
    use serde_json::Value;

    use crate::log;

    #[pyfunction]
    pub fn print(text: String) {
        let _ = log::println(&text);
    }

    #[derive(Debug)]
    pub struct Instance {
        sender: Sender<FrameData>,
    }

    pub static INSTANCE: OnceCell<Instance> = OnceCell::new();

    pub fn initialize(sender: Sender<FrameData>) {
        INSTANCE
            .set(Instance { sender })
            .expect("initialize failed");
    }

    pub struct FrameData {
        pub action: String,
        pub name: String,
        pub value: Value,
    }

    impl<'a> TryFromBorrowedObject<'a> for FrameData {
        fn try_from_borrowed_object(vm: &VirtualMachine, obj: &'a PyObject) -> PyResult<Self> {
            let action = obj.get_attr("action", vm)?.try_into_value::<String>(vm)?;
            let value = obj.get_attr("value", vm)?.try_into_value::<String>(vm)?;
            let name = obj.get_attr("name", vm)?.try_into_value::<String>(vm)?;
            Ok(FrameData {
                action,
                value: serde_json::from_str(&value)
                    .map_err(|e| vm.new_value_error(e.to_string()))?,
                name,
            })
        }
    }

    #[pyfunction]
    pub fn send(data: FrameData) {
        let _ = INSTANCE.get().unwrap().sender.send(data);
    }

    #[pyfunction]
    pub fn fetch(method: String, url: String, vm: &VirtualMachine) -> PyResult<String> {
        executor::block_on(async {
            a_fetch(method, url)
                .await
                .map_err(|e| vm.new_runtime_error(e.to_string()))
        })
    }

    #[pyfunction]
    pub fn reload_scripts() {
        send(FrameData { action: "reload".to_string(), name: "reload".to_owned(), value: Value::Null });
    }

    async fn a_fetch(method: String, url: String) -> Result<String> {
        let response;
        if method == "GET" {
            response = reqwest::get(url).await?.text().await?;
        } else {
            return Err(color_eyre::eyre::Error::msg("method incorrect"));
        }
        Ok(response)
    }
}
