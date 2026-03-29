use indicatif::{ProgressBar, ProgressStyle};
use reqwest::blocking::Client;
use serde::Deserialize;
use std::env;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::process;
use std::process::Command;
use std::thread;
use std::time::{Duration, SystemTime};
use zip::ZipArchive;

// ─── Version embebida en compile-time ────────────────────────────────
const APP_VERSION: &str = env!("CARGO_PKG_VERSION");
const GITHUB_API_URL: &str = "https://api.github.com";
const GITHUB_OWNER: &str = "wertyMSD";

// ─── Estructuras para la API de GitHub ───────────────────────────────
#[derive(Debug, Deserialize)]
struct GitHubRelease {
    tag_name: String,
    name: Option<String>,
    assets: Vec<GitHubAsset>,
    published_at: String,
    prerelease: bool,
}

#[derive(Debug, Deserialize)]
struct GitHubAsset {
    name: String,
    browser_download_url: String,
    size: u64,
    content_type: Option<String>,
}

// ─── Reemplazo del ejecutable (batch script) ─────────────────────────
fn programar_reemplazo_exe(
    ruta_actual: &Path,
    ruta_nueva: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let ruta_backup = ruta_actual.with_extension("bak");
    let script_path = ruta_actual.with_extension("update.bat");

    let contenido_script = format!(
        "@echo off\r\n\
         setlocal\r\n\
         set SRC=\"{}\"\r\n\
         set DEST=\"{}\"\r\n\
         set BACK=\"{}\"\r\n\
         ping 127.0.0.1 -n 2 >nul\r\n\
         if exist %BACK% del /f /q %BACK%\r\n\
         if exist %DEST% move /y %DEST% %BACK%\r\n\
         move /y %SRC% %DEST%\r\n\
         start \"\" %DEST%\r\n\
         del \"%~f0\"\r\n",
        ruta_nueva.display(),
        ruta_actual.display(),
        ruta_backup.display()
    );

    fs::write(&script_path, contenido_script)?;
    Command::new("cmd")
        .args(&["/C", &script_path.to_string_lossy()])
        .spawn()?;
    Ok(())
}

// ─── Comparación semántica de versiones ──────────────────────────────
fn comparar_versiones(nueva: &str, actual: &str) -> bool {
    let parse = |v: &str| -> Vec<u32> {
        v.trim_start_matches('v')
            .split(|c: char| !c.is_ascii_digit())
            .filter_map(|part| part.parse::<u32>().ok())
            .collect()
    };

    let partes_nueva = parse(nueva);
    let partes_actual = parse(actual);

    for i in 0..std::cmp::max(partes_nueva.len(), partes_actual.len()) {
        let n = partes_nueva.get(i).unwrap_or(&0);
        let a = partes_actual.get(i).unwrap_or(&0);
        if n > a {
            return true;
        }
        if n < a {
            return false;
        }
    }
    false
}

// ─── Cliente HTTP reutilizable ───────────────────────────────────────
fn crear_cliente_http() -> Result<Client, Box<dyn std::error::Error>> {
    let client = Client::builder()
        .timeout(Duration::from_secs(120))
        .connect_timeout(Duration::from_secs(15))
        .user_agent("updategit")
        .build()?;
    Ok(client)
}

// ─── GitHub API: obtener última release ──────────────────────────────
fn obtener_ultima_release(
    client: &Client,
    owner: &str,
    repo: &str,
    token: Option<&str>,
) -> Result<GitHubRelease, Box<dyn std::error::Error>> {
    let url = format!(
        "{}/repos/{}/{}/releases/latest",
        GITHUB_API_URL, owner, repo
    );

    let mut request = client
        .get(&url)
        .header("Accept", "application/vnd.github.v3+json")
        .header("User-Agent", "updategit");

    if let Some(t) = token {
        request = request.header("Authorization", format!("Bearer {}", t));
    }

    let response = request.send()?;

    if response.status() == reqwest::StatusCode::NOT_FOUND {
        return Err(format!(
            "No se encontraron releases en {}/{}. Verifique que el repositorio exista y tenga releases publicadas.",
            owner, repo
        )
        .into());
    }

    let response = response.error_for_status()?;
    let release: GitHubRelease = response.json()?;
    Ok(release)
}

// ─── Auto-actualización del propio updategit via GitHub Releases ─────
fn intentar_auto_actualizacion(
    client: &Client,
    owner: &str,
    repo: &str,
    token: Option<&str>,
    ruta_ejecutable_actual: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let nombre_exe = ruta_ejecutable_actual
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or("No se pudo obtener el nombre del ejecutable actual")?
        .to_lowercase();

    let release = match obtener_ultima_release(client, owner, repo, token) {
        Ok(r) => r,
        Err(_) => {
            println!("No se pudo verificar auto-actualización (sin releases disponibles).");
            return Ok(());
        }
    };

    let necesita_actualizar = comparar_versiones(&release.tag_name, APP_VERSION);

    if !necesita_actualizar {
        println!(
            "Versión actual {} es la más reciente (remota: {}). No se requiere auto-actualización.",
            APP_VERSION, release.tag_name
        );
        return Ok(());
    }

    // Buscar el binario del updater en los assets de la release
    let asset = release.assets.iter().find(|a| {
        let name_lower = a.name.to_lowercase();
        name_lower == nombre_exe
            || name_lower == format!("{}.exe", nombre_exe)
            || (name_lower.starts_with("updategit")
                && (name_lower.ends_with(".exe") || !name_lower.contains('.')))
    });

    let asset = match asset {
        Some(a) => a,
        None => {
            println!(
                "No se encontró el asset '{}' en la release {}. Saltando auto-actualización.",
                nombre_exe, release.tag_name
            );
            return Ok(());
        }
    };

    println!(
        "Se encontró una nueva versión {} (actual: {}). Actualizando...",
        release.tag_name, APP_VERSION
    );

    let ruta_descarga = ruta_ejecutable_actual.with_extension("tmp");
    descargar_archivo_con_progreso(
        client,
        &asset.browser_download_url,
        &ruta_descarga,
        asset.size,
    )?;

    programar_reemplazo_exe(ruta_ejecutable_actual, &ruta_descarga)?;
    println!("Auto-actualización descargada. Se reiniciará con la nueva versión.");
    process::exit(0);
}

// ─── Descarga HTTP con barra de progreso ─────────────────────────────
fn descargar_archivo_con_progreso(
    client: &Client,
    url: &str,
    destino: &Path,
    tamano_total: u64,
) -> Result<(), Box<dyn std::error::Error>> {
    let pb = ProgressBar::new(tamano_total);
    pb.set_style(
        ProgressStyle::with_template(
            "[{elapsed_precise}] [{bar:40.cyan/blue}] {bytes}/{total_bytes} ({eta})",
        )?
        .progress_chars("##-"),
    );

    let mut response = client.get(url).send()?.error_for_status()?;
    let mut archivo = File::create(destino)?;
    let mut buffer = [0u8; 8192];
    let mut descargado: u64 = 0;

    loop {
        let bytes_leidos = response.read(&mut buffer)?;
        if bytes_leidos == 0 {
            break;
        }
        archivo.write_all(&buffer[..bytes_leidos])?;
        descargado += bytes_leidos as u64;
        pb.set_position(descargado);
    }

    pb.finish_with_message("Descarga completa");
    Ok(())
}

// ─── Buscar asset en release por nombre base ─────────────────────────
fn buscar_asset_por_nombre<'a>(
    release: &'a GitHubRelease,
    nombre_base: &str,
) -> Option<&'a GitHubAsset> {
    let base_lower = nombre_base.to_lowercase();

    // 1) Coincidencia exacta: nombrebase.zip o nombrebase_version.zip
    let exacto = release.assets.iter().find(|a| {
        let n = a.name.to_lowercase();
        n == format!("{}.zip", base_lower)
            || n == format!(
                "{}_{}.zip",
                base_lower,
                release.tag_name.to_lowercase().trim_start_matches('v')
            )
            || n == format!(
                "{}-{}.zip",
                base_lower,
                release.tag_name.to_lowercase().trim_start_matches('v')
            )
    });
    if exacto.is_some() {
        return exacto;
    }

    // 2) Coincidencia parcial: contiene el nombre base y termina en .zip
    let parcial = release.assets.iter().find(|a| {
        let n = a.name.to_lowercase();
        n.contains(&base_lower) && n.ends_with(".zip")
    });
    if parcial.is_some() {
        return parcial;
    }

    // 3) Fallback: usar solo la primera parte antes de '_' o '-'
    let primer_componente = base_lower
        .split(|c: char| c == '_' || c == '-')
        .next()
        .unwrap_or(&base_lower);

    if primer_componente != base_lower {
        let fallback = release.assets.iter().find(|a| {
            let n = a.name.to_lowercase();
            n.contains(primer_componente) && n.ends_with(".zip")
        });
        if fallback.is_some() {
            return fallback;
        }
    }

    None
}

// ─── Mover archivos de un directorio a otro recursivamente ───────────
fn mover_archivos_al_raiz(dir_origen: &str, dir_destino: &str) -> Result<usize, std::io::Error> {
    let mut archivos_movidos = 0;
    let origen_path = Path::new(dir_origen);

    if !origen_path.exists() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            format!("Directorio origen no existe: {}", dir_origen),
        ));
    }

    fn mover_recursivo(
        origen: &Path,
        destino: &Path,
        contador: &mut usize,
    ) -> Result<(), std::io::Error> {
        for entry in std::fs::read_dir(origen)? {
            let entry = entry?;
            let ruta_origen = entry.path();
            let nombre_archivo = ruta_origen.file_name().ok_or_else(|| {
                std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    "Nombre de archivo inválido",
                )
            })?;

            let ruta_destino = destino.join(&nombre_archivo);

            if ruta_origen.is_dir() {
                std::fs::create_dir_all(&ruta_destino)?;
                mover_recursivo(&ruta_origen, &ruta_destino, contador)?;
            } else {
                if ruta_destino.exists() {
                    std::fs::remove_file(&ruta_destino)?;
                }
                std::fs::rename(&ruta_origen, &ruta_destino)?;
                *contador += 1;
            }
        }
        Ok(())
    }

    mover_recursivo(origen_path, Path::new(dir_destino), &mut archivos_movidos)?;

    if origen_path.exists() {
        std::fs::remove_dir_all(origen_path)?;
    }

    Ok(archivos_movidos)
}

// ─── Esperar a que un archivo esté disponible para lectura ───────────
fn esperar_archivo_disponible(
    nombre_archivo: &str,
    max_espera_segundos: u64,
) -> Result<(), String> {
    let mut intentos = 0;
    let max_intentos = max_espera_segundos * 2;

    while intentos < max_intentos {
        match OpenOptions::new()
            .read(true)
            .write(false)
            .create(false)
            .open(nombre_archivo)
        {
            Ok(_) => match File::open(nombre_archivo) {
                Ok(mut file) => {
                    let mut buffer = [0u8; 4];
                    match file.read_exact(&mut buffer) {
                        Ok(_) => return Ok(()),
                        Err(_) => {}
                    }
                }
                Err(_) => {}
            },
            Err(e) => {
                if e.kind() == io::ErrorKind::PermissionDenied {
                    // Archivo en uso por otro proceso
                } else {
                    return Err(format!("No se puede acceder al archivo: {}", e));
                }
            }
        }

        intentos += 1;
        thread::sleep(Duration::from_millis(500));
    }

    Err(format!(
        "El archivo {} no está disponible después de {} segundos",
        nombre_archivo, max_espera_segundos
    ))
}

// ─── Descomprimir ZIP (con o sin contraseña) ─────────────────────────
pub fn descomprimir_zip(
    nombre_archivo: &str,
    password: &str,
    destino: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let archivo_zip = File::open(nombre_archivo)?;
    let mut archivo = ZipArchive::new(archivo_zip)?;

    for i in 0..archivo.len() {
        let mut archivo_comprimido = if password.is_empty() {
            // Sin contraseña
            match archivo.by_index(i) {
                Ok(f) => f,
                Err(e) => {
                    return Err(format!("Error al abrir entrada {} del ZIP: {}", i, e).into());
                }
            }
        } else {
            // Con contraseña (AES)
            match archivo.by_index_decrypt(i, password.as_bytes()) {
                Ok(f) => f,
                Err(e) => {
                    return Err(format!(
                        "Error al descomprimir entrada {} (¿contraseña incorrecta?): {}",
                        i, e
                    )
                    .into());
                }
            }
        };

        let ruta_archivo = match archivo_comprimido.enclosed_name() {
            Some(path) => path.to_owned(),
            None => continue,
        };

        let ruta_final = Path::new(destino).join(&ruta_archivo);

        if archivo_comprimido.is_dir() {
            fs::create_dir_all(&ruta_final)?;
        } else {
            if let Some(parent) = ruta_final.parent() {
                fs::create_dir_all(parent)?;
            }

            if ruta_final.exists() {
                fs::remove_file(&ruta_final)?;
            }

            let mut archivo_salida = File::create(&ruta_final)?;
            io::copy(&mut archivo_comprimido, &mut archivo_salida)?;
        }
    }

    Ok(())
}

// ─── Finalizar proceso: ocultar _internal y lanzar la app ────────────
fn finalizar_proceso(
    error_ocurrido: bool,
    motivo_error: String,
    ruta_absoluta_exe: PathBuf,
) -> Result<(), Box<dyn std::error::Error>> {
    if !ruta_absoluta_exe.exists() {
        eprintln!(
            "Error grave: No se encuentra el fichero ejecutable: {}",
            ruta_absoluta_exe.display()
        );
        process::exit(1);
    }

    let folder_path = "_internal";
    let _output = Command::new("attrib")
        .arg("+h")
        .arg(folder_path)
        .output()
        .expect("Error al ejecutar el comando attrib");

    if error_ocurrido {
        eprintln!("El proceso finalizó con errores. Motivo: {}", motivo_error);
        println!("Ejecutando con error: {}", ruta_absoluta_exe.display());
        let _resultado = process::Command::new("cmd")
            .args(&[
                "/C",
                &ruta_absoluta_exe.to_string_lossy(),
                "-no",
                &motivo_error,
            ])
            .spawn();

        process::exit(1);
    } else {
        println!("Ejecutando: {}", ruta_absoluta_exe.display());
        let _resultado = process::Command::new("cmd")
            .args(&["/C", &ruta_absoluta_exe.to_string_lossy()])
            .spawn();
    }

    Ok(())
}

// ─── Parsear "owner/repo" ────────────────────────────────────────────
fn parsear_repo(repo_str: &str) -> Result<(&str, &str), Box<dyn std::error::Error>> {
    let parts: Vec<&str> = repo_str.splitn(2, '/').collect();
    if parts.len() != 2 || parts[0].is_empty() || parts[1].is_empty() {
        return Err(format!(
            "Formato de repositorio inválido: '{}'. Use formato: owner/repo",
            repo_str
        )
        .into());
    }
    Ok((parts[0], parts[1]))
}

// ─── Punto de entrada principal ──────────────────────────────────────
fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Mostrar información del programa
    println!("╔══════════════════════════════════════════════════════════╗");
    println!("║       UPDATEGIT - ACTUALIZADOR DESDE GITHUB RELEASE      ║");
    println!(
        "║                  Versión {} - AMG                    ║",
        APP_VERSION
    );
    println!("║                  © 2025 AMG                          ║");
    println!("╚══════════════════════════════════════════════════════════╝");
    println!();

    let args: Vec<String> = env::args().collect();

    // ── Flags (extraer antes de parsear posicionalmente) ───────────────
    let self_update = args.iter().any(|a| a == "--self-update")
        || env::var("SELF_UPDATE")
            .map(|v| v == "true")
            .unwrap_or(false);

    let positional: Vec<&String> = args
        .iter()
        .skip(1)
        .filter(|a| !a.starts_with("--"))
        .collect();

    // ── Argumento 1: nombre de la app (default "s50info") ─────────────
    let nombre_archivo = positional
        .get(0)
        .map(|s| s.to_lowercase())
        .unwrap_or_else(|| "s50info".to_string());
    println!("APP procesando: {}", nombre_archivo);

    // ── Argumento 2: nombre del repositorio GitHub ────────────────────
    // (se resuelve después de calcular nombre_base)
    let repo_arg = positional.get(1).map(|s| (*s).clone());

    // ── Token GitHub (opcional, para repos privados o mayor rate limit)
    let github_token = env::var("GITHUB_TOKEN").ok();

    // ── Nombre base (sin extensión) ──────────────────────────────────
    let nombre_base = match nombre_archivo.find('.') {
        Some(idx) => nombre_archivo[..idx].to_string(),
        None => nombre_archivo.clone(),
    };

    // ── Repositorio GitHub (default = nombre_base) ───────────────────
    let repo =
        repo_arg.unwrap_or_else(|| env::var("GITHUB_REPO").unwrap_or_else(|_| nombre_base.clone()));
    let owner = GITHUB_OWNER;
    println!("GitHub repo: {}/{}", owner, repo);

    // ── Ruta del ejecutable de la app a lanzar ───────────────────────
    let mut ruta = env::current_exe()?
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .to_path_buf();

    // Si estamos dentro de "temp", subir un nivel
    if ruta.ends_with("temp") {
        if let Some(parent) = ruta.parent() {
            ruta = parent.to_path_buf();
        }
    }

    let ruta_absoluta_exe = ruta.join(format!("{}.exe", &nombre_base));
    println!("Ruta final: {}", ruta_absoluta_exe.display());

    // ── Matar proceso de la app si está corriendo ─────────────────────
    let exe_name = format!("{}.exe", &nombre_base);
    println!("Verificando si {} está corriendo...", exe_name);
    let kill_output = Command::new("taskkill")
        .args(&["/F", "/IM", &exe_name])
        .output();

    match kill_output {
        Ok(output) => {
            if output.status.success() {
                println!("Proceso {} terminado.", exe_name);
            } else {
                let stderr = String::from_utf8_lossy(&output.stderr);
                if stderr.contains("no se encontró")
                    || stderr.contains("not found")
                    || stderr.contains("ERROR")
                {
                    println!(
                        "{} no está corriendo, no es necesario terminarlo.",
                        exe_name
                    );
                } else {
                    println!("No se pudo verificar {}: {}", exe_name, stderr.trim());
                }
            }
        }
        Err(e) => println!("No se pudo ejecutar taskkill: {}", e),
    }
    thread::sleep(Duration::from_secs(1));

    let mut error_ocurrido = false;
    let mut motivo_error: String = String::new();
    let posponer_path = Path::new("posponer.update");

    // ── Mecanismo de posposición (24h) ───────────────────────────────
    if posponer_path.exists() {
        let mut posponer_activo = false;

        if let Ok(metadata) = fs::metadata(posponer_path) {
            if let Ok(fecha) = metadata.modified() {
                if let Ok(transcurrido) = SystemTime::now().duration_since(fecha) {
                    if transcurrido < Duration::from_secs(24 * 60 * 60) {
                        posponer_activo = true;
                    }
                }
            }
        }

        if posponer_activo {
            println!(
                "Actualización pospuesta: se volverá a intentar pasada 24 horas desde {}.",
                posponer_path.display()
            );
            return finalizar_proceso(false, String::new(), ruta_absoluta_exe);
        } else if let Err(e) = fs::remove_file(posponer_path) {
            eprintln!(
                "No se pudo eliminar el marcador de posposición {}: {}",
                posponer_path.display(),
                e
            );
        } else {
            println!(
                "Marcador de posposición antiguo eliminado: {}",
                posponer_path.display()
            );
        }
    }

    // ── Borrar archivo ZIP viejo si ya existe la app ─────────────────
    let dir_path = Path::new("./_internal");
    let archivo_path = Path::new(&nombre_archivo);

    if archivo_path.exists() && dir_path.exists() {
        if let Err(e) = fs::remove_file(&archivo_path) {
            eprintln!(
                "Error al borrar el archivo existente {}: {}",
                nombre_archivo, e
            );
            error_ocurrido = true;
            motivo_error = format!("Error al borrar el archivo existente: {}", e);
            return finalizar_proceso(error_ocurrido, motivo_error, ruta_absoluta_exe);
        } else {
            println!("Archivo existente {} borrado.", nombre_archivo);
        }
    } else {
        println!(
            "El archivo {} no existe previamente. Se procederá a descargarlo.",
            nombre_archivo
        );
    }

    // ── Crear cliente HTTP ───────────────────────────────────────────
    let client = crear_cliente_http().map_err(|e| format!("Error al crear cliente HTTP: {}", e))?;

    // ── Verificar conectividad con GitHub ────────────────────────────
    println!("Verificando conexión con GitHub...");
    match client.get("https://api.github.com").send() {
        Ok(resp) if resp.status().is_success() => {
            println!("Conexión a GitHub exitosa.")
        }
        Ok(resp) => {
            let status = resp.status();
            eprintln!("GitHub respondió con estado: {}", status);
            error_ocurrido = true;
            motivo_error = format!("GitHub respondió con estado: {}", status);
            return finalizar_proceso(error_ocurrido, motivo_error, ruta_absoluta_exe);
        }
        Err(e) => {
            eprintln!("Error al conectar a GitHub: {}", e);
            error_ocurrido = true;
            motivo_error = format!("Error al conectar a GitHub: {}", e);
            return finalizar_proceso(error_ocurrido, motivo_error, ruta_absoluta_exe);
        }
    }

    // ── Auto-actualización del propio updategit (solo con --self-update) ─
    if self_update {
        let self_repo_str =
            env::var("SELF_UPDATE_REPO").unwrap_or_else(|_| format!("{}/updategit", GITHUB_OWNER));
        let (self_owner, self_repo) = parsear_repo(&self_repo_str)?;

        println!("Verificando auto-actualización...");
        if let Ok(ruta_ejecutable_actual) = env::current_exe() {
            if let Err(e) = intentar_auto_actualizacion(
                &client,
                self_owner,
                self_repo,
                github_token.as_deref(),
                &ruta_ejecutable_actual,
            ) {
                eprintln!("No se pudo comprobar auto-actualización: {}", e);
            }
        } else {
            eprintln!("No se pudo obtener la ruta del ejecutable actual para auto-actualización.");
        }

        println!("Auto-actualización completada.");
        return Ok(());
    }

    // ── Obtener última release del repo de la app ────────────────────
    println!("\nBuscando última release en {}/{}...", owner, repo);
    let release = match obtener_ultima_release(&client, owner, &repo, github_token.as_deref()) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("{}", e);
            error_ocurrido = true;
            motivo_error = e.to_string();
            return finalizar_proceso(error_ocurrido, motivo_error, ruta_absoluta_exe);
        }
    };

    println!(
        "Última release encontrada: {} ({})",
        release.tag_name, release.published_at
    );
    println!("Assets disponibles:");
    for asset in &release.assets {
        println!("  - {} ({} bytes)", asset.name, asset.size);
    }

    // ── Buscar el asset que coincida con la app ──────────────────────
    let asset = match buscar_asset_por_nombre(&release, &nombre_base) {
        Some(a) => a,
        None => {
            eprintln!(
                "No se encontró un asset ZIP que coincida con '{}' en la release {}",
                nombre_base, release.tag_name
            );
            error_ocurrido = true;
            motivo_error = format!(
                "No se encontró asset ZIP para '{}' en release {}",
                nombre_base, release.tag_name
            );
            return finalizar_proceso(error_ocurrido, motivo_error, ruta_absoluta_exe);
        }
    };

    let nombre_archivo_final = &asset.name;
    println!(
        "\nDescargando: {} ({} bytes)",
        nombre_archivo_final, asset.size
    );

    // ── Descargar con barra de progreso ──────────────────────────────
    if let Err(e) = descargar_archivo_con_progreso(
        &client,
        &asset.browser_download_url,
        Path::new(nombre_archivo_final),
        asset.size,
    ) {
        eprintln!("Error al descargar el archivo: {}", e);
        error_ocurrido = true;
        motivo_error = format!("Error al descargar el archivo: {}", e);
        return finalizar_proceso(error_ocurrido, motivo_error, ruta_absoluta_exe);
    }

    // Asegurar que el archivo está completamente cerrado
    println!("\nEsperando a que el archivo se libere completamente...");
    thread::sleep(Duration::from_secs(2));

    // ── Estrategia: descompresión en carpeta temporal → mover al raíz ─
    println!(
        "\nIniciando descompresión segura para: {}",
        nombre_archivo_final
    );

    let temp_zip_dir = "temp_zip";
    println!("Creando directorio temporal: {}", temp_zip_dir);

    if Path::new(temp_zip_dir).exists() {
        println!("Eliminando directorio temporal existente...");
        if let Err(e) = fs::remove_dir_all(temp_zip_dir) {
            eprintln!("Error al eliminar directorio temporal: {}", e);
        }
    }

    if let Err(e) = fs::create_dir_all(temp_zip_dir) {
        eprintln!("Error al crear directorio temporal: {}", e);
        error_ocurrido = true;
        motivo_error = format!("Error al crear directorio temporal: {}", e);
        return finalizar_proceso(error_ocurrido, motivo_error, ruta_absoluta_exe);
    }
    println!("Directorio temporal creado exitosamente");

    println!("Esperando 2 segundos para asegurar que el archivo ZIP esté liberado...");
    thread::sleep(Duration::from_secs(2));

    // ── Descompresión con reintentos ─────────────────────────────────
    let password = env::var("ZIP_PASSWORD").unwrap_or_else(|_| "123".to_string());
    let max_intentos = 3;
    let mut descompresion_exitosa = false;
    let mut ultimo_error: Option<String> = None;

    for intento in 1..=max_intentos {
        println!("\nIntento de descompresión {}/{}", intento, max_intentos);

        match esperar_archivo_disponible(nombre_archivo_final, 5) {
            Ok(_) => {
                println!("Archivo ZIP disponible para descompresión");
                thread::sleep(Duration::from_millis(500));

                println!("Descomprimiendo en carpeta temporal: {}", temp_zip_dir);
                match descomprimir_zip(nombre_archivo_final, &password, temp_zip_dir) {
                    Ok(_) => {
                        println!("Descompresión completada en directorio temporal");

                        let temp_path = Path::new(temp_zip_dir);
                        let archivos_descomprimidos = match std::fs::read_dir(temp_path) {
                            Ok(entries) => entries.count(),
                            Err(_) => 0,
                        };

                        if archivos_descomprimidos > 0 {
                            println!(
                                "Se encontraron {} archivos/directorios descomprimidos",
                                archivos_descomprimidos
                            );
                            descompresion_exitosa = true;
                            ultimo_error = None;
                            break;
                        } else {
                            println!("No se encontraron archivos descomprimidos");
                            ultimo_error = Some(
                                "No se encontraron archivos después de la descompresión"
                                    .to_string(),
                            );
                        }
                    }
                    Err(e) => {
                        let mensaje_error = e.to_string();
                        println!("Error en descompresión: {}", mensaje_error);
                        ultimo_error = Some(mensaje_error);

                        if intento < max_intentos {
                            println!(
                                "Esperando {} segundos antes del próximo intento...",
                                intento * 5
                            );
                            thread::sleep(Duration::from_secs(intento * 5));
                        }
                    }
                }
            }
            Err(err) => {
                println!("Error al verificar disponibilidad: {}", err);
                ultimo_error = Some(err);

                if intento < max_intentos {
                    println!(
                        "Esperando {} segundos antes del próximo intento...",
                        intento * 5
                    );
                    thread::sleep(Duration::from_secs(intento * 5));
                }
            }
        }
    }

    // ── Si la descompresión fue exitosa, mover archivos al raíz ──────
    if descompresion_exitosa {
        println!("\nIniciando movimiento de archivos al raíz...");

        // Verificar permisos de escritura
        println!("\nVerificando permisos necesarios...");
        match std::fs::write("test_permisos.txt", "permisos_test") {
            Ok(_) => {
                let _ = std::fs::remove_file("test_permisos.txt");
                println!("Permisos de escritura verificados");
            }
            Err(e) => {
                println!("Error de permisos: {}", e);
                println!("POSIBLE SOLUCIÓN:");
                println!("   1. Ejecutar como administrador");
                println!("   2. Desactivar antivirus temporalmente");
                println!("   3. Revisar permisos de la carpeta");
                error_ocurrido = true;
                motivo_error = format!("Error de permisos: {}", e);
                return finalizar_proceso(error_ocurrido, motivo_error, ruta_absoluta_exe);
            }
        }

        match mover_archivos_al_raiz(temp_zip_dir, ".") {
            Ok(archivos_movidos) => {
                println!(
                    "Se movieron {} archivos/directorios al raíz exitosamente",
                    archivos_movidos
                );

                // Limpiar ZIP y directorio temporal
                println!("\nForzando limpieza de archivos temporales...");

                match fs::remove_file(nombre_archivo_final) {
                    Ok(_) => {
                        println!("Archivo ZIP eliminado: {}", nombre_archivo_final);
                    }
                    Err(e) => {
                        println!(
                            "Error al eliminar archivo ZIP {}: {}",
                            nombre_archivo_final, e
                        );
                    }
                }

                match fs::remove_dir_all(temp_zip_dir) {
                    Ok(_) => {
                        println!("Directorio temporal eliminado: {}", temp_zip_dir);
                    }
                    Err(e) => {
                        println!(
                            "Error al eliminar directorio temporal {}: {}",
                            temp_zip_dir, e
                        );
                        println!("Nota: Esto puede ocurrir si archivos están en uso");
                        println!("Los archivos movidos permanecerán en el directorio raíz");
                    }
                }

                // Verificación final
                println!("\nVerificación final de archivos...");
                if Path::new("_internal").exists()
                    || Path::new(&format!("{}.exe", nombre_base)).exists()
                {
                    println!("Verificación final exitosa - Archivos principales encontrados");
                } else {
                    println!("Advertencia: No se encontraron los archivos principales esperados");
                    println!("\nArchivos encontrados en el directorio actual:");
                    if let Ok(entries) = fs::read_dir(".") {
                        for entry in entries {
                            if let Ok(entry) = entry {
                                let path = entry.path();
                                if path.is_file() {
                                    println!("   {}", path.display());
                                } else if path.is_dir() {
                                    println!("   {}", path.display());
                                }
                            }
                        }
                    }
                }
            }
            Err(e) => {
                println!("Error al mover archivos al raíz: {}", e);
                error_ocurrido = true;
                motivo_error = format!("Error al mover archivos: {}", e);

                match fs::remove_dir_all(temp_zip_dir) {
                    Ok(_) => {
                        println!(
                            "Directorio temporal eliminado a pesar del error: {}",
                            temp_zip_dir
                        );
                    }
                    Err(err_borrado) => {
                        println!(
                            "No se pudo eliminar el directorio temporal {}: {}",
                            temp_zip_dir, err_borrado
                        );
                    }
                }

                if e.to_string().to_lowercase().contains("access denied") {
                    println!("\nSOLUCIONES POSIBLES:");
                    println!("   1. Ejecutar como administrador");
                    println!("   2. Desactivar antivirus temporalmente");
                    println!("   3. Revisar permisos de la carpeta");
                    println!("   4. Cerrar programas que usen archivos");
                }
            }
        }
    } else {
        println!(
            "La descompresión falló después de {} intentos",
            max_intentos
        );
        error_ocurrido = true;
        motivo_error = if let Some(ref err) = ultimo_error {
            format!("No se pudo descomprimir el archivo. Último error: {}", err)
        } else {
            "No se pudo descomprimir el archivo por razones desconocidas".to_string()
        };

        // Crear marcador de posposición
        if let Err(e) = fs::write(posponer_path, "pospuesto por errores de descompresión\n") {
            eprintln!("No se pudo crear marcador de posposición: {}", e);
        } else {
            println!("Marcador de posposición creado para reintentar en 24 horas");
        }
    }

    // Limpiar el ZIP descargado
    if fs::remove_file(nombre_archivo_final).is_ok() {
        println!("Archivo eliminado: {}", nombre_archivo_final);
    }

    finalizar_proceso(error_ocurrido, motivo_error, ruta_absoluta_exe)
}
