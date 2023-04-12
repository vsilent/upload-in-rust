use std::collections::HashMap;
use bytes::BufMut;
use futures::{StreamExt, TryStreamExt};
use std::convert::Infallible;
use std::fs;
use std::path::Path;
use uuid::Uuid;
use warp::{http::StatusCode, http::HeaderMap, multipart::{FormData, Part}, Filter, Rejection, Reply, Stream};

#[tokio::main]
async fn main() {
    let cors = warp::cors()
        .allow_any_origin()
        .allow_headers(vec!["User-Agent", "Sec-Fetch-Mode", "Referer", "Origin",
                            "Access-Control-Request-Method", "Access-Control-Request-Headers",
                            "Content-Type", "Accept", "Authorization", "content-type", "type"])
        .allow_methods(vec!["GET", "POST", "DELETE", "OPTIONS"]);

    // let options_only = warp::any().and(warp::options().map(warp::reply).with(&cors));
    let upload_route = warp::path("upload")
        .and(log_headers())
        .and(warp::post())
        .and(warp::multipart::form().max_length(10_000_000))
        .and_then(upload)
        .with(&cors);

    let download_route = warp::path("file")
        .and(warp::fs::dir("./files/")
            .with(&cors));

    let list_route = warp::path("files")
        .and(warp::get())
        .and_then(list)
        .with(&cors);

    let delete_route = warp::path!("files" / String)
        .and(log_headers())
        .and(warp::delete())
        .and_then(delete)
        .with(&cors);

    let router = upload_route
        .or(download_route)
        .or(list_route)
        .or(delete_route)
        .recover(handle_rejection);

    let port = 8080;
    println!("Server started at localhost: {}", &port);

    warp::serve(router).run(([0, 0, 0, 0], port)).await;
}

fn log_headers() -> impl Filter<Extract=(), Error=Infallible> + Copy {
    warp::header::headers_cloned()
        .map(|headers: HeaderMap| {
            for (k, v) in headers.iter() {
                // Error from `to_str` should be handled properly
                println!("{}: {}", k, v.to_str().expect("Failed to print header value"))
            }
        })
        .untuple_one()
}

async fn get_extension(content_type: Option<String>) -> String {
    let mut file_ending;
    match content_type {
        Some(file_type) => match file_type.as_str() {
            "application/pdf" => {
                file_ending = "pdf";
            }
            "image/svg+xml" => {
                file_ending = "svg";
            }
            "image/png" => {
                file_ending = "png";
            }
            "image/jpeg" => {
                file_ending = "jpg";
            }
            "image/gif" => {
                file_ending = "gif";
            }
            "image/bmp" => {
                file_ending = "bmp";
            }
            "image/webp" => {
                file_ending = "webp";
            }
            "image/x-icon" => {
                file_ending = "ico";
            }
            "image/vnd.microsoft.icon" => {
                file_ending = "ico";
            }
            "image/x-ms-bmp" => {
                file_ending = "bmp";
            }
            _ => {
                file_ending = "bin";
            }
        }
        _ => {
            file_ending = "bin";
        }
    }
    file_ending.to_string()
}

async fn upload(mut form: warp::multipart::FormData) -> Result<impl Reply, Rejection> {
    let mut response = HashMap::new();

    while let Some(part_result) = form.next().await {
        let mut ext = String::from("");
        match part_result {
            Ok(p) => {
                // Process the file part here
                if p.name() == "file" {
                    if let Some(ct) = p.content_type() {
                        ext = get_extension(Some(ct.to_string())).await;
                    }
                }
                println!("Received file part: {:?}", &p);
                let value = p.stream().try_fold(Vec::new(), |mut vec, data| {
                    vec.put(data);
                    async move { Ok(vec) }
                })
                    .await
                    .map_err(|e| {
                        eprintln!("reading file error: {}", e);
                        warp::reject::reject()
                    })?;

                let file_name = format!("{}.{}", Uuid::new_v4().to_string(), ext);
                let file_path = format!("./files/{}", file_name);
                tokio::fs::write(&file_path, value).await.map_err(|e| {
                    eprint!("error writing file: {}", e);
                    warp::reject::reject()
                })?;
                println!("created file: {}", &file_path);
                response.insert("filename", file_name);
            }
            Err(err) => {
                // Handle any errors that may occur during reading
                eprintln!("Error reading file part: {}", err);
                return Err(warp::reject::reject());
            }
        }
    }

    response.insert("status", StatusCode::OK.to_string());
    Ok(warp::reply::json(&response))
}

async fn list() -> Result<impl Reply, Rejection> {
    let paths = fs::read_dir("./files").unwrap();
    // let files: Vec<_> = paths.map(|res| res.unwrap().path().file_name()).collect();
    let files = paths.filter_map(|entry| {
        entry.ok().and_then(|e|
            e.path().file_name()
                .and_then(|n| n.to_str().map(|s| String::from(s)))
        )
    }).collect::<Vec<String>>();
    Ok(warp::reply::json(&files))
}

async fn delete(file_name: String) -> Result<impl Reply, Rejection> {
    let mut response = HashMap::new();
    let file_path = format!("./files/{}", file_name);
    println!("Deleting file {:?}", file_path);

    if Path::new(&file_path).exists() {
        std::fs::remove_file(&file_path).unwrap();
        response.insert("status", StatusCode::OK.to_string());
    } else {
        response.insert("status", StatusCode::NOT_FOUND.to_string());
    }

    Ok(warp::reply::json(&response))
}

async fn handle_rejection(err: Rejection) -> std::result::Result<impl Reply, Infallible> {
    let (code, message) = if err.is_not_found() {
        (StatusCode::NOT_FOUND, "Not Found".to_string())
    } else if err.find::<warp::reject::PayloadTooLarge>().is_some() {
        (StatusCode::BAD_REQUEST, "Payload too large".to_string())
    } else {
        eprintln!("unhandled error: {:?}", err);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            "Internal Server Error".to_string(),
        )
    };

    Ok(warp::reply::with_status(message, code))
}