use anyhow::Result;
use deno_core::{extension, op2, Extension};

use crate::log;

extension!(op_std, ops = [op_println],);

#[op2(async)]
async fn op_println(#[string] content: String) -> Result<()> {
    log::println(&content)?;
    Ok(())
}

extension!(op_http, ops = [op_http_get]);

#[op2(async)]
#[string]
async fn op_http_get(#[string] url: String) -> Result<String> {
    let response = reqwest::get(url).await?;
    let text = response.text().await?;
    Ok(text)
}

pub fn get_extensions() -> Vec<Extension> {
    vec![op_std::init_ops(), op_http::init_ops()]
}
