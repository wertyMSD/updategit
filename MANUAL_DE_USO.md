# MANUAL DE USO - UPDATEGIT

**Version 1.2.0 | (c) 2025 ALCA TIC, S.L.**

---

## 1. Requisitos previos

- **Sistema operativo**: Windows (el programa utiliza comandos nativos de Windows como `attrib`, `ping` y scripts `.bat` para el reemplazo en caliente del ejecutable).
- **Conectividad a Internet**: acceso a `api.github.com` y a `github.com` (dominios de descarga de assets).
- **Permisos de escritura**: el directorio de trabajo debe permitir lectura, escritura, creacion y eliminacion de archivos.
- **Ejecutable de la aplicacion**: se asume que el `.exe` principal de la aplicacion destino (`<nombre_app>.exe`) reside en el mismo directorio que `updategit.exe`.

No se requiere instalacion de runtime adicional; el binario compilado es completamente autonomo (enlazado estaticamente con Rust).

---

## 2. Uso basico

### Sintaxis

```
updategit <nombre_app> <owner/repo>
```

| Parametro       | Descripcion                                                       | Obligatorio |
|-----------------|-------------------------------------------------------------------|-------------|
| `nombre_app`    | Nombre base de la aplicacion (sin extension `.exe`). Si se omite, el valor por defecto es `s50info`. | No |
| `owner/repo`    | Repositorio de GitHub en formato `propietario/repositorio`        | Si (salvo que se defina `GITHUB_REPO`) |

### Ejemplos

```bat
updategit
```
Actualiza `s50info` usando el repositorio indicado en la variable de entorno `GITHUB_REPO`.

```bat
updategit miapp
```
Actualiza la aplicacion `miapp` (buscara `miapp.zip` o `miapp.exe` en la release) usando el repo de la variable de entorno `GITHUB_REPO`.

```bat
updategit miapp miorganizacion/mirepo
```
Actualiza `miapp` consultando las releases del repositorio `miorganizacion/mirepo` en GitHub.

```bat
set GITHUB_REPO=miorganizacion/mirepo
updategit contabilidad
```
Igual que el anterior, pero el repositorio se indica mediante variable de entorno.

---

## 3. Variables de entorno

### `GITHUB_REPO`

Repositorio de GitHub de la aplicacion a actualizar, en formato `owner/repo`. Se utiliza como valor por defecto cuando no se pasa el segundo argumento en la linea de comandos.

```bat
set GITHUB_REPO=miorganizacion/mirepo
```

### `GITHUB_TOKEN`

Token de acceso personal de GitHub (PAT). Su uso es opcional y proporciona dos beneficios:

- Acceso a releases de **repositorios privados**.
- Un limite de tasa (rate limit) mas elevado en la API de GitHub (5000 peticiones/hora en vez de 60).

```bat
set GITHUB_TOKEN=ghp_xxxxxxxxxxxxxxxxxxxx
```

### `SELF_UPDATE_REPO`

Repositorio desde el cual `updategit` verifica si existe una nueva version de si mismo. Por defecto toma el mismo valor que `GITHUB_REPO`. Se utiliza cuando `updategit` reside en un repositorio diferente al de la aplicacion que actualiza.

```bat
set SELF_UPDATE_REPO=miorganizacion/updategit
```

### `ZIP_PASSWORD`

Contrasena para descomprimir archivos ZIP cifrados con AES. El valor por defecto es `123`. Si el ZIP no esta protegido, la contrasena se ignora internamente.

```bat
set ZIP_PASSWORD=micontrasena
```

---

## 4. Flujo de trabajo

Al ejecutar `updategit`, el programa sigue este orden de operaciones:

### 4.1 Verificacion de pospuesto

Comprueba si existe el archivo `posponer.update` en el directorio de trabajo. Si existe y su antiguedad es inferior a 24 horas, se omite la actualizacion y se lanza directamente la aplicacion existente. Si ha pasado mas de 24 horas, el marcador se elimina y el proceso continua normalmente.

### 4.2 Limpieza del ZIP anterior

Si el archivo ZIP de la aplicacion y la carpeta `_internal` existen previamente, se elimina el archivo ZIP viejo antes de iniciar la descarga.

### 4.3 Conexion a GitHub API

Se realiza una peticion GET a `https://api.github.com` para verificar la conectividad. Si la respuesta no es exitosa, el programa aborta y lanza la aplicacion existente con indicador de error.

### 4.4 Auto-actualizacion del propio updategit

Se consulta la ultima release del repositorio indicado en `SELF_UPDATE_REPO`. Si la version remota (etiqueta `tag_name`) es superior a la version embebida en compile-time (`CARGO_PKG_VERSION`), se descarga el nuevo binario y se programa su reemplazo. El programa termina con `process::exit(0)` para que el script `.bat` pueda sustituir el ejecutable. En la siguiente ejecucion se usara ya la version actualizada.

Si no se puede verificar la auto-actualizacion (por ejemplo, no hay releases disponibles), se continua normalmente con el flujo.

### 4.5 Busqueda de la ultima release

Se consulta el endpoint `/repos/{owner}/{repo}/releases/latest` de la API de GitHub para obtener la release mas reciente (no pre-release) del repositorio de la aplicacion.

### 4.6 Seleccion del asset

Se busca entre los assets de la release aquel cuyo nombre coincida con el parametro `nombre_app` (ver seccion 7 para los criterios de busqueda detallados).

### 4.7 Descarga con barra de progreso

Se descarga el archivo ZIP del asset seleccionado usando HTTP GET. Durante la descarga se muestra una barra de progreso con el tama~no total, bytes descargados y tiempo estimado restante (ETA). La descarga se realiza en bloques de 8 KB y se escribe directamente a disco.

### 4.8 Espera de liberacion del archivo

Tras la descarga, se esperan 2 segundos adicionales para garantizar que el sistema operativo libere completamente el handle del archivo.

### 4.9 Descompresion en carpeta temporal

Los archivos se descomprimen en una carpeta llamada `temp_zip`. El programa realiza hasta 3 intentos de descompresion con esperas progresivas (5, 10 y 15 segundos). Antes de cada intento, verifica que el archivo ZIP este disponible para lectura (sin bloqueos por otros procesos).

### 4.10 Movimiento al directorio raiz

Los archivos descomprimidos se mueven recursivamente desde `temp_zip` al directorio de trabajo actual. Si un archivo destino ya existe, se elimina antes de mover el nuevo. Las subcarpetas se crean automaticamente. Finalmente, se elimina el directorio temporal `temp_zip`.

### 4.11 Verificacion y limpieza

- Se oculta la carpeta `_internal` con el comando `attrib +h`.
- Se elimina el archivo ZIP descargado.
- Se verifica que existan los archivos principales (`_internal` o `<nombre_app>.exe`).

### 4.12 Lanzamiento de la aplicacion

Si todo fue correcto, se lanza la aplicacion descargada con `cmd /C <ruta_exe>`. Si hubo errores en cualquier paso, se lanza con el parametro `-no <motivo_error>` para que la aplicacion pueda mostrar un mensaje al usuario.

---

## 5. Auto-actualizacion

### 5.1 Como funciona

`updategit` se actualiza a si mismo comparando su version embebida en tiempo de compilacion (`CARGO_PKG_VERSION`, actualmente `1.2.0`) con la etiqueta `tag_name` de la ultima release disponible en el repositorio definido por `SELF_UPDATE_REPO`.

### 5.2 Comparacion semantica de versiones

La funcion de comparacion realiza un parseo semantico:

1. Se elimina el prefijo `v` si esta presente (por ejemplo, `v1.3.0` se convierte en `1.3.0`).
2. Se separa por caracteres no numericos y se extraen los componentes como enteros.
3. Se comparan componente a componente de forma numerica (`major.minor.patch`).
4. Si todos los componentes son iguales, se considera que no hay actualizacion.

Solo se actualiza si la version remota es **estrictamente mayor** que la local.

### 5.3 Busqueda del asset para auto-actualizacion

En la release remota, se busca un asset que cumpla alguno de estos criterios:

- Nombre exacto del ejecutable actual (en minusculas).
- Nombre del ejecutable con extension `.exe`.
- Cualquier nombre que empiece por `updategit` y termine en `.exe` (o no contenga punto).

### 5.4 Reemplazo en caliente

El proceso de reemplazo utiliza un archivo batch (`.bat`) con la siguiente logica:

1. El nuevo ejecutable se descarga con extension `.tmp` junto al ejecutable actual.
2. Se genera un script `.bat` que realiza: espera 2 segundos (via `ping`), crea backup `.bak`, mueve el nuevo ejecutable a la posicion del actual, y lanza el nuevo ejecutable.
3. El script se ejecuta como proceso separado con `cmd /C`.
4. El programa original termina con `process::exit(0)`.
5. El script `.bat` se auto-elimina tras su ejecucion.

---

## 6. Formato de los assets en la release de GitHub

La release de GitHub debe contener al menos un archivo ZIP con la aplicacion. El nombre del asset se busca siguiendo este orden de prioridad:

### 6.1 Coincidencia exacta

Se buscan estos patrones (en minusculas):

| Patron                         | Ejemplo con `miapp` y tag `v2.1.0` |
|-------------------------------|-------------------------------------|
| `<nombre>.zip`                | `miapp.zip`                         |
| `<nombre>_<version>.zip`      | `miapp_2.1.0.zip`                  |
| `<nombre>-<version>.zip`      | `miapp-2.1.0.zip`                  |

### 6.2 Coincidencia parcial

Si no se encuentra coincidencia exacta, se busca cualquier asset cuyo nombre contenga el `nombre_app` y termine en `.zip`.

### 6.3 Notas

- La comparacion es **insensible a mayusculas y minusculas**.
- El `nombre_app` se trunca en el primer punto si se incluye extension (por ejemplo, `miapp.zip` se convierte en `miapp` para la busqueda).
- Si no se encuentra ningun asset que coincida, el programa muestra un error con la lista de assets disponibles y lanza la aplicacion existente.

---

## 7. Resolucion de errores

### 7.1 Mecanismo de pospuesta (24 horas)

Cuando la descompresion falla tras los 3 intentos permitidos, el programa:

1. Crea un archivo llamado `posponer.update` en el directorio de trabajo.
2. Lanza la aplicacion existente con indicador de error.
3. En la siguiente ejecucion, si el archivo `posponer.update` existe y tiene menos de 24 horas, se omite toda la logica de actualizacion y se lanza la aplicacion directamente.
4. Transcurridas 24 horas, el archivo se elimina automaticamente y el proceso de actualizacion se reintenta.

### 7.2 Reintentos de descompresion

La descompresion del ZIP se intenta hasta 3 veces con una estrategia de espera progresiva:

- **Intento 1**: espera 5 segundos antes del siguiente intento si falla.
- **Intento 2**: espera 10 segundos antes del siguiente intento si falla.
- **Intento 3**: ultimo intento. Si falla, se activa la pospuesta de 24 horas.

Antes de cada intento, se verifica que el archivo ZIP este disponible para lectura mediante `OpenOptions::new().read(true)`, reintentando cada 500ms durante 5 segundos.

### 7.3 Verificacion de permisos

Antes de mover los archivos al directorio raiz, el programa realiza una prueba de escritura creando un archivo `test_permisos.txt`. Si esta operacion falla, se muestran las siguientes soluciones sugeridas:

1. Ejecutar como administrador.
2. Desactivar el antivirus temporalmente.
3. Revisar los permisos de la carpeta.

Si el movimiento de archivos falla con un error de "access denied", se sugieren soluciones adicionales como cerrar programas que esten usando los archivos.

### 7.4 Errores de conectividad

Si no se puede conectar a `api.github.com`, el programa muestra el estado de la respuesta HTTP o el error de conexion, y procede a lanzar la aplicacion existente con el parametro `-no` y el motivo del error.

### 7.5 Repositorio o release no encontrados

Si el endpoint de releases devuelve HTTP 404, el programa informa de que no se encontraron releases y sugiere verificar que el repositorio exista y tenga releases publicadas.

---

## 8. Cambios respecto a la version FTP anterior

La version actual de `updategit` sustituye completamente el mecanismo de descarga basado en FTP por GitHub Releases. Los cambios principales son:

| Aspecto                        | Version FTP anterior                     | Version actual (GitHub Releases)             |
|-------------------------------|-----------------------------------------|---------------------------------------------|
| Protocolo de descarga          | FTP                                     | HTTP/HTTPS via `reqwest`                    |
| Fuente de archivos            | Servidor FTP                            | GitHub Releases API                         |
| Autenticacion                 | Credenciales FTP                        | Token GitHub (opcional, via `GITHUB_TOKEN`) |
| Deteccion de nuevas versiones | Basada en fecha o nombre de archivo     | Comparacion semantica de versiones          |
| Rate limiting                 | No aplicable                            | 60 req/h (sin token), 5000 req/h (con token) |
| Cifrado de archivos           | Dependia del servidor FTP               | ZIP con cifrado AES (via `zip` crate)       |
| Barra de progreso             | No disponible                           | Si, con `indicatif`                         |
| Auto-actualizacion            | No disponible                           | Si, con reemplazo en caliente via `.bat`    |
| Sistema de pospuesta          | No disponible                           | Si, archivo `posponer.update` (24 horas)    |
| Reintentos automaticos        | No disponibles                          | Hasta 3 intentos de descompresion           |
| Verificacion de permisos      | No disponible                           | Si, test de escritura previo                |

### Dependencias principales (Cargo.toml)

| Crate     | Uso                                                |
|-----------|----------------------------------------------------|
| `reqwest` | Cliente HTTP blocking con soporte TLS (rustls)     |
| `serde`   | Deserializacion de respuestas JSON de la API       |
| `indicatif`| Barra de progreso en consola                       |
| `zip`     | Lectura y descompresion de ZIP con soporte AES      |
