use std::{panic, time::SystemTime};

use color_eyre::{config::HookBuilder, eyre};

use crate::{log, tui};

/// This replaces the standard color_eyre panic and error hooks with hooks that
/// restore the terminal before printing the panic or error.
pub fn install_hooks() -> color_eyre::Result<()> {
    let (_panic_hook, eyre_hook) = HookBuilder::default().into_hooks();

    // convert from a color_eyre PanicHook to a standard panic hook
    // let panic_hook = panic_hook.into_panic_hook();
    panic::set_hook(Box::new(move |panic_info| {
        // tui::restore().unwrap();
        // panic_hook(panic_info);
        let time = SystemTime::now();
        let _ = log::println(&format!("Panic: {:?}: {:?}", time, panic_info));
    }));

    // convert from a color_eyre EyreHook to a eyre ErrorHook
    let eyre_hook = eyre_hook.into_eyre_hook();
    eyre::set_hook(Box::new(
        move |error: &(dyn std::error::Error + 'static)| {
            tui::restore().unwrap();
            let time = SystemTime::now();
            let _ = log::println(&format!("Eyre: {:?}: {:?}", time, error));
            eyre_hook(error)
        },
    ))?;

    Ok(())
}
