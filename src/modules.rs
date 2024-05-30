use boa_engine::{js_string, Context, JsArgs, JsError, JsNativeError, JsResult, JsValue};

use crate::log;

fn e(x: reqwest::Error) -> JsError {
    JsNativeError::error().with_message(x.to_string()).into()
}

pub fn fetch(_: &JsValue, args: &[JsValue], _context: &mut Context) -> JsResult<JsValue> {
    let default_method = js_string!("GET");
    let method = args
        .get_or_undefined(0)
        .as_string()
        .unwrap_or(&default_method);
    let url = args
        .get_or_undefined(1)
        .as_string()
        .ok_or_else(|| JsNativeError::error().with_message("no url"))?;
    let method = method.to_std_string_escaped();
    let response;
    if method.to_uppercase() == "GET" {
        response = reqwest::blocking::get(url.to_std_string_escaped())
            .map_err(e)?
            .text()
            .map_err(e)?;
    } else {
        return Err(JsNativeError::error().with_message("check method").into());
    }
    Ok(JsValue::String(js_string!(response)))
}

pub fn print(_: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    log::println(&format!("{:?}", args[0].to_string(context)?)).unwrap();
    Ok(JsValue::Undefined)
}
