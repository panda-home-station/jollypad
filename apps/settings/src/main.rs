use slint::{ComponentHandle, VecModel, Timer};
use std::rc::Rc;
use std::cell::Cell;
use jollypad_core::CatacombClient;
use catacomb_ipc::{IpcMessage, WindowScale};

slint::include_modules!();

fn main() -> Result<(), slint::PlatformError> {
    let ui = SettingsWindow::new()?;
    
    // Set role for window manager if needed
    CatacombClient::set_system_role("settings", "JollyPad-Settings");
    
    // Build resolution list from current output info
    let (pw, ph, current_refresh, current_scale, _orientation) = CatacombClient::get_output_info()
        .unwrap_or((1920, 1080, 60000, 1.0, catacomb_ipc::Orientation::Landscape));
    
    // Resolution Setup
    let modes = CatacombClient::get_output_modes();
    let modes = if modes.is_empty() {
        vec![catacomb_ipc::OutputMode { width: pw, height: ph, refresh: current_refresh }]
    } else {
        modes
    };

    let mut mode_items: Vec<slint::SharedString> = Vec::new();
    let mut current_mode_index = 0;

    for (i, mode) in modes.iter().enumerate() {
        let refresh_rate = mode.refresh as f64 / 1000.0;
        mode_items.push(format!("{}x{} @ {:.2}Hz", mode.width, mode.height, refresh_rate).into());
        
        if mode.width == pw && mode.height == ph && mode.refresh == current_refresh {
            current_mode_index = i as i32;
        }
    }

    ui.set_resolution_options(Rc::new(VecModel::from(mode_items)).into());
    ui.set_resolution_popup_index(current_mode_index);
    
    if !modes.is_empty() {
         let current_mode = &modes[current_mode_index as usize];
         ui.set_current_resolution(format!("{}x{} @ {:.2}Hz", current_mode.width, current_mode.height, current_mode.refresh as f64 / 1000.0).into());
    } else {
         ui.set_current_resolution(format!("{pw}×{ph}").into());
    }

    // Confirmation State
    let confirmed_mode_index = Rc::new(Cell::new(current_mode_index as usize));
    let timer = Rc::new(Timer::default());

    // Apply Resolution
    {
        let modes = modes.clone();
        let ui_weak = ui.as_weak();
        let confirmed_idx = confirmed_mode_index.clone();
        let timer = timer.clone();
        
        ui.on_apply_resolution(move |idx| {
            let idx = idx.clamp(0, (modes.len() - 1) as i32) as usize;
            let mode = &modes[idx];
            
            // Apply immediately
            let _ = CatacombClient::send_message(IpcMessage::SetOutputMode { mode: mode.clone() });
            
            if let Some(ui) = ui_weak.upgrade() {
                 ui.set_current_resolution(format!("{}x{} @ {:.2}Hz", mode.width, mode.height, mode.refresh as f64 / 1000.0).into());
                 ui.set_countdown_seconds(15);
                 ui.set_show_confirmation_popup(true);
            }

            let ui_weak_timer = ui_weak.clone();
            let timer_handle = timer.clone();
            let modes_timer = modes.clone();
            let confirmed_idx_timer = confirmed_idx.clone();

            timer.start(slint::TimerMode::Repeated, std::time::Duration::from_secs(1), move || {
                if let Some(ui) = ui_weak_timer.upgrade() {
                    let mut s = ui.get_countdown_seconds();
                    s -= 1;
                    ui.set_countdown_seconds(s);
                    
                    if s <= 0 {
                        // Revert
                        let idx = confirmed_idx_timer.get();
                        let mode = &modes_timer[idx];
                        let _ = CatacombClient::send_message(IpcMessage::SetOutputMode { mode: mode.clone() });
                        ui.set_current_resolution(format!("{}x{} @ {:.2}Hz", mode.width, mode.height, mode.refresh as f64 / 1000.0).into());
                        ui.set_show_confirmation_popup(false);
                        ui.set_resolution_popup_index(idx as i32);
                        timer_handle.stop();
                    }
                }
            });
        });
    }

    // Confirm Resolution
    {
        let confirmed_idx = confirmed_mode_index.clone();
        let timer = timer.clone();
        let ui_weak = ui.as_weak();
        
        ui.on_confirm_resolution(move || {
            timer.stop();
            if let Some(ui) = ui_weak.upgrade() {
                ui.set_show_confirmation_popup(false);
                let current_idx = ui.get_resolution_popup_index();
                confirmed_idx.set(current_idx as usize);
            }
        });
    }

    // Revert Resolution
    {
        let modes = modes.clone();
        let confirmed_idx = confirmed_mode_index.clone();
        let timer = timer.clone();
        let ui_weak = ui.as_weak();
        
        ui.on_revert_resolution(move || {
            timer.stop();
            if let Some(ui) = ui_weak.upgrade() {
                let idx = confirmed_idx.get();
                let mode = &modes[idx];
                let _ = CatacombClient::send_message(IpcMessage::SetOutputMode { mode: mode.clone() });
                ui.set_current_resolution(format!("{}x{} @ {:.2}Hz", mode.width, mode.height, mode.refresh as f64 / 1000.0).into());
                ui.set_show_confirmation_popup(false);
                ui.set_resolution_popup_index(idx as i32);
            }
        });
    }

    let scales: [f64; 5] = [1.0, 1.25, 1.5, 1.75, 2.0];
    let mut items: Vec<slint::SharedString> = Vec::new();
    let mut current_index = 0;
    let mut min_diff = f64::MAX;

    for (i, &s) in scales.iter().enumerate() {
        items.push(format!("{:.0}%", s * 100.0).into());

        let diff = (s - current_scale).abs();
        if diff < min_diff {
            min_diff = diff;
            current_index = i as i32;
        }
    }
    ui.set_scale_options(Rc::new(VecModel::from(items)).into());
    ui.set_current_scale(format!("{:.0}%", scales[current_index as usize] * 100.0).into());
    ui.set_popup_index(current_index);
    
    {
        let scales = scales;
        let ui_weak = ui.as_weak();
        ui.on_apply_scale(move |idx| {
            let idx = idx.clamp(0, (scales.len() - 1) as i32) as usize;
            let scale = scales[idx];
            let _ = CatacombClient::send_message(IpcMessage::Scale { scale: WindowScale::Fixed(scale), app_id: None });
            
            if let Some(ui) = ui_weak.upgrade() {
                ui.set_current_scale(format!("{:.0}%", scale * 100.0).into());
            }
        });
    }
    
    ui.on_close_requested({
        move || {
            std::process::exit(0);
        }
    });

    // Gamepad Input Support (Removed - now handled by Catacomb via System Role)

    ui.run()
}
