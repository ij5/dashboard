// use rustpython_vm::{self as vm};
// use vm::pymodule;

use anyhow::Result;
use futures::executor;
use rustpython_vm::{scope::Scope, PyResult, VirtualMachine};

use crate::log;

// #[pymodule]
// pub mod std {
//     use rustpython_vm::{function::PosArgs, PyResult, VirtualMachine};

//     use crate::log;

//     #[pyfunction]
//     pub fn print(objects: PosArgs, vm: &VirtualMachine) -> PyResult<()> {
//         let mut result = String::new();
//         for object in objects {
//             result.push_str(" ");
//             result.push_str(object.str(vm)?.as_str());
//         }
//         let _ = log::println(&result);
//         Ok(())
//     }
// }

pub fn build_modules(vm: &VirtualMachine) -> PyResult<Scope> {
    let scope = vm.new_scope_with_builtins();
    let print_fn = vm.new_function("print", print);
    scope.globals.set_item("print", print_fn.into(), &vm)?;
    let fetch_fn = vm.new_function("fetch", fetch);
    scope.globals.set_item("fetch", fetch_fn.into(), &vm)?;
    Ok(scope)
}

pub fn print(text: String) {
    let _ = log::println(&text);
}

pub fn fetch(method: String, url: String, vm: &VirtualMachine) -> PyResult<String> {
    executor::block_on(async {
      a_fetch(method, url).await.map_err(|e| vm.new_runtime_error(e.to_string()))
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