#![no_main]

use jni::objects::{JObject, JValue};
use jni::JavaVM;
use log::{info, LevelFilter};
use ndk_context::android_context;
use std::os::raw::c_void;
use log::warn;
use log::error;

// Importação do Slint para Android
#[cfg(target_os = "android")]
slint::slint! {
    import { VerticalBox, LineEdit } from "std-widgets.slint";
    export component MainWindow inherits Window {
        in property <string> gps-text: "Aguardando dados de GPS...";
        
        VerticalBox {
            LineEdit {
                text: gps-text;
                read-only: true;
            }
        }
    }
}

#[unsafe(no_mangle)]
fn android_main(app: slint::android::AndroidApp) {
    // Inicialização do logger
    info!("[GPS Thread] Iniciando thread para obtenção de coordenadas...");
    android_logger::init_once(
        android_logger::Config::default().with_max_level(LevelFilter::Info)
    );
    info!("Inicializando app Slint com GPS");

    // Inicialização do Slint
    slint::android::init(app).unwrap();

    let window = MainWindow::new().unwrap();

    // Atualização assíncrona das coordenadas GPS
    let window_weak = window.as_weak();

    std::thread::spawn(move || {
        info!("[GPS Thread] Thread de GPS iniciada");
        
        match get_gps_coordinates() {
            Ok((lat, lon)) => {
                info!("[GPS Thread] Coordenadas obtidas com sucesso: lat={:.6}, lon={:.6}", lat, lon);
                let text = format!("Latitude: {:.4}, Longitude: {:.4}", lat, lon);
                
                if let Some(window) = window_weak.upgrade() {
                    info!("[GPS Thread] Atualizando UI com as coordenadas");
                    window.set_gps_text(text.into());
                } else {
                    warn!("[GPS Thread] Window já foi destruída, não é possível atualizar UI");
                }
            }
            Err(e) => {
                error!("[GPS Thread] Erro ao obter coordenadas: {}", e);
                let error_text = format!("Erro: {}", e);
                
                if let Some(window) = window_weak.upgrade() {
                    warn!("[GPS Thread] Atualizando UI com mensagem de erro");
                    window.set_gps_text(error_text.into());
                } else {
                    warn!("[GPS Thread] Window já foi destruída, não é possível mostrar erro");
                }
            }
        }
        
        info!("[GPS Thread] Finalizando thread de GPS");
    });

    window.run().unwrap();
}

fn get_gps_coordinates() -> Result<(f64, f64), Box<dyn std::error::Error>> {
    let ctx = android_context();
    let vm_ptr = ctx.vm() as *mut jni::sys::JavaVM;
    let env_ptr = ctx.context() as *mut c_void;

    // Verificação de ponteiros nulos
    if vm_ptr.is_null() || env_ptr.is_null() {
        return Err("Contexto Android não disponível".into());
    }

    let vm = unsafe { JavaVM::from_raw(vm_ptr)? };
    let mut env = vm.attach_current_thread()?;

    let context = unsafe { JObject::from_raw(env_ptr as jni::sys::jobject) };

    // Verificação de permissões
    let permission_str = env.new_string("android.permission.ACCESS_FINE_LOCATION")?;
    let has_permission = env
        .call_method(
            &context,
            "checkSelfPermission",
            "(Ljava/lang/String;)I",
            &[JValue::Object(&JObject::from(permission_str))],
        )?
        .i()?;

    if has_permission != 0 { // PERMISSION_GRANTED = 0
        return Err("Permissão de localização não concedida".into());
    }

    // Obtenção do serviço de localização
    let service_str = env.new_string("location")?;
    let location_service = env
        .call_method(
            &context,
            "getSystemService",
            "(Ljava/lang/String;)Ljava/lang/Object;",
            &[JValue::Object(&JObject::from(service_str))],
        )?
        .l()?;

    if location_service.is_null() {
        return Err("Serviço de localização não disponível".into());
    }

    let location_manager = JObject::from(location_service);
    let provider_str = env.new_string("gps")?;

    // Verificação se o provedor GPS está habilitado
    let is_enabled = env
        .call_method(
            &location_manager,
            "isProviderEnabled",
            "(Ljava/lang/String;)Z",
            &[JValue::Object(&JObject::from(provider_str))],
        )?
        .z()?;

    if !is_enabled {
        return Err("Provedor GPS está desativado".into());
    }

    // Obtenção da última localização conhecida
    let gps_str = env.new_string("gps")?;
    let location_obj = env
        .call_method(
            &location_manager,
            "getLastKnownLocation",
            "(Ljava/lang/String;)Landroid/location/Location;",
            &[JValue::Object(&JObject::from(gps_str))],
        )?
        .l()?;

    if location_obj.is_null() {
        return Err("Nenhuma localização conhecida disponível".into());
    }

    // Extração das coordenadas
    let latitude = env
        .call_method(&location_obj, "getLatitude", "()D", &[])?
        .d()?;

    let longitude = env
        .call_method(&location_obj, "getLongitude", "()D", &[])?
        .d()?;

    Ok((latitude, longitude))
}