use core::{panic, result::Result};
use std::{
    env::args, io::Result as io_result, path::Path, process::exit, sync::Arc, time::Duration, vec,
};

use bincode::{deserialize, serialize};
use dashmap::DashSet;
use reqwest::{Client, Error, StatusCode};
use scraper::{Html, Selector};
use tokio::{
    fs::{self, read},
    sync::mpsc::{self, Receiver, Sender},
    task::{self, JoinSet},
    time::sleep,
};
use url::Url;

async fn download<T: AsRef<[u8]>>(
    url: &Url,
    dir: Arc<String>,
    content: T,
    pages: bool,
) -> io_result<()> {
    let the_fn = url.path();
    let path = Path::new(the_fn);
    let dir = dir.to_string();
    let mut tail_dir = String::from("/");
    let mut final_path = dir.to_owned() + the_fn;
    if pages && !the_fn.ends_with(".html") {
        tail_dir = tail_dir + path.file_name().unwrap().to_str().unwrap();
        final_path = final_path + "/index.html";
    }
    if let Some(parent) = path.parent() {
        if parent.to_str() != Some("") {
            fs::create_dir_all(dir + parent.to_str().unwrap() + &tail_dir).await?;
        }
    }
    fs::write(final_path, content).await?;
    Ok(())
}

fn decrement(sender: Arc<Sender<Url>>) {
    let s = Arc::strong_count(&sender);
    let ptr = Arc::into_raw(sender);
    unsafe {
        if s == 2 {
            Arc::decrement_strong_count(ptr);
        }
        Arc::decrement_strong_count(ptr);
    }
}

#[derive(Debug)]
enum FetchError {
    ReqwestError(Error),
    StatusCode(StatusCode),
}

enum FetchedContent {
    StringContent(String),
    VectContent(Vec<u8>),
}

impl From<Error> for FetchError {
    fn from(err: Error) -> FetchError {
        FetchError::ReqwestError(err)
    }
}

impl FetchedContent {
    fn as_str(self) -> String {
        match self {
            Self::StringContent(s) => s,
            Self::VectContent(_) => {
                panic!("Called as_str() on an FetchedContent::VectContent variant")
            }
        }
    }
}

async fn fetch(
    url_str: &str,
    client: Arc<Client>,
    bytes: bool,
    pages: bool,
) -> Result<FetchedContent, FetchError> {
    let res = client.get(url_str).send().await?;
    let status = res.status();
    if status != StatusCode::OK {
        return Err(FetchError::StatusCode(status));
    }
    let content = if bytes {
        (&*res.bytes().await?).to_vec()
    } else {
        let dom = res.text().await?;
        if pages {
            return Ok(FetchedContent::StringContent(dom));
        }
        dom.into_bytes()
    };
    Ok(FetchedContent::VectContent(content))
}

async fn download_wrapper<T: AsRef<[u8]>>(
    url: &Url,
    dir: Arc<String>,
    content: T,
    pages: bool,
    fail_safe: Arc<DashSet<String>>,
) {
    if let Err(err) = download(&url, dir, content, pages).await {
        eprintln!("\x1b[1;91mError downloading\x1b[0m {} {err}", &url);
        fail_safe.insert(url.to_string());
    }
}

async fn download_resources(
    rx: &mut Receiver<Url>,
    client: &Arc<Client>,
    dir: &Arc<String>,
    bytes: bool,
    duration: u64,
    failed: &Arc<DashSet<String>>,
) {
    let total = rx.len();
    let mut progress = 0;
    let text = if bytes {
        String::from("image") + if total == 1 { "" } else { "s" }
    } else {
        String::from("js & css files")
    };
    println!("\x1b[1;93mdownloading {total} {text}\x1b[0m");
    let mut set = JoinSet::new();
    while let Some(url) = rx.recv().await {
        if duration != 0 {
            sleep(Duration::from_millis(duration)).await;
        }
        let c = client.clone();
        let dir = dir.clone();
        let fail = failed.clone();
        progress += 1;
        set.spawn(async move {
            let url_str = url.as_str();
            let fetched_content = fetch(url_str, c, bytes, false).await;
            if let Ok(content) = fetched_content {
                println!("\x1b[96m{}/{}\x1b[0m {}", progress, total, url_str);
                match content {
                    FetchedContent::StringContent(value) => {
                        download_wrapper(&url, dir, value, false, fail).await
                    }
                    FetchedContent::VectContent(value) => {
                        download_wrapper(&url, dir, value, false, fail).await
                    }
                };
            } else if let Err(FetchError::StatusCode(status)) = fetched_content {
                fail_log(
                    format!(
                        "\x1b[1;91m{}\x1b[0m \x1b[96m{}/{}\x1b[0m {}",
                        status, progress, total, url_str
                    ),
                    fail,
                    url,
                );
            } else if let Err(FetchError::ReqwestError(err)) = fetched_content {
                fail_log(
                    format!(
                        "\x1b[1;91mError downloading\x1b[0m \x1b[96m{}/{}\x1b[0m {}, {}",
                        progress, total, url_str, err
                    ),
                    fail,
                    url,
                );
            }
        });
    }
    set.join_all().await;
}

fn exit_code_1(msg: String) -> ! {
    eprintln!("{}", msg);
    exit(1)
}

fn fail_log(msg: String, failed: Arc<DashSet<String>>, url: Url) {
    eprintln!("{}", msg);
    failed.insert(url.to_string());
}

fn stringify_urls(urls_set: Arc<DashSet<String>>, init: String) -> String {
    urls_set.iter().fold(init, |acc, x| acc + x.as_str() + "\n")
}

async fn deserialize_file(log_file: &str) -> String {
    if let Ok(log) = read(Path::new(log_file)).await {
        deserialize::<String>(&log).unwrap()
    } else {
        exit_code_1(format!(
            "Error occured while attempting to read ./{}. Does this file exist?",
            log_file
        ))
    }
}

#[tokio::main]
async fn main() {
    let (tx, mut rx) = mpsc::channel(1000); // TEST: this with 5 or so.
    let (imgs_tx, mut imgs_rx) = mpsc::channel(10000); // TODO: warn about this in this limit in the
                                                       // readme. But check if there's any
                                                       // downsides for encreasing this value
    let (js_css_tx, mut js_css_rx) = mpsc::channel(10000);
    let tx = Arc::new(tx);
    let imgs_tx = Arc::new(imgs_tx);
    let js_css_tx = Arc::new(js_css_tx);
    let client = Arc::new(Client::new());
    let urls = Arc::new(DashSet::new());
    let failed = Arc::new(DashSet::new());
    let f_imgs = Arc::new(DashSet::new());
    let f_js_css = Arc::new(DashSet::new());

    let mut arguments: Vec<String> = args().collect();
    let duration = if arguments.len() == 5 {
        let delay = arguments.pop().unwrap();
        if let Ok(d) = delay.parse() {
            d
        } else {
            exit_code_1(format!("{} is invalid argument", delay)); // TODO: clearify that this is yhe
        }
    } else {
        0
    };
    let [dir, base_url] = if arguments.len() == 2 && arguments[1] == "-a" {
        let missed_urls = deserialize_file("failed-log.bin").await;
        let visited_urls = deserialize_file("visited-log-urls.bin").await;
        for v in visited_urls.lines() {
            urls.insert(v.to_string());
        }
        let mut categories = missed_urls.split("----");
        let base_url = Arc::new(categories.next().unwrap().to_string());
        for c in categories {
            let the_tx = if c.starts_with("js_css\n") {
                &js_css_tx
            } else if c.starts_with("imgs\n") {
                &imgs_tx
            } else {
                &tx
            };
            let mut missed_urls = c.lines();
            missed_urls.next();
            for url in missed_urls {
                if let Err(_) = the_tx.send(Url::parse(url).unwrap()).await {
                    exit_code_1(format!("Receiver dropped"));
                }
            }
        }
        [Arc::new(String::from(".")), base_url]
    } else {
        let args = match arguments.len() {
            4 => {
                let d = arguments.remove(3);
                let b = arguments.remove(2);
                let u = arguments.remove(1);
                [u, b, d]
            }
            _ => {
                exit_code_1(format!("Usage: {} url base dir", arguments[0]));
            }
        };

        let [url, dir, base] = args;

        let dir = Arc::new(dir);

        let base_index = if let Ok(bi) = base.parse() {
            bi
        } else {
            exit_code_1(format!("{} is invalid argument", base)); // TODO: clearify that this is yhe
                                                                  // base argument
        };

        let url = if let Ok(url) = Url::parse(&url) {
            url
        } else {
            exit_code_1(format!("the url is not valid"));
        };

        let d = &*dir;
        if let Ok(false) = fs::try_exists(d).await {
            if let Err(err) = fs::create_dir(d).await {
                exit_code_1(format!("err creating {d}: {err}"));
            };
        }

        let p = Path::new(url.path()).iter().take(base_index + 1);
        if p.clone().count() <= base_index {
            exit_code_1(format!("url path is less than base {}", base_index));
        }

        let base_path = p.fold("".to_string(), |acc, e| {
            let slash = if e == "/" || acc == "/" { "" } else { "/" };
            acc + slash + e.to_str().unwrap()
        }) + if base_index == 0 { "" } else { "/" }; // TODO: can I make the 'fold' method above include this trailig '/' I added in this
                                                     // line. Solving thr bug of (base: "http://xxx/a", urls :["http://xxx/a/b";"http://xxx/ab/b"...]).
                                                     // Create a new stash and new commit for this bug fix.
        let base_url = url.join(&base_path).unwrap().to_string();
        let base_url = Arc::new(base_url);

        urls.insert(url.to_string());
        // WARN: tx.clone() here is not necessary, so I removed it from the `cargo run -- -a` block above, but it could affect Arc::strong_count, test it before removing it here.
        if let Err(_) = tx.clone().send(url).await {
            exit_code_1(format!("Receiver dropped"));
        }
        [dir, base_url]
    };

    let mut set = JoinSet::new();
    while let Some(url) = rx.recv().await {
        let c = client.clone();
        let sender = Arc::clone(&tx);
        let imgs_sender = Arc::clone(&imgs_tx);
        let js_css_sender = Arc::clone(&js_css_tx);
        let urls = Arc::clone(&urls);
        let base_url = Arc::clone(&base_url);
        let dir = Arc::clone(&dir);
        let fail = Arc::clone(&failed);

        if duration != 0 {
            sleep(Duration::from_millis(duration)).await;
        }

        set.spawn(async move {
            let url_str = url.as_str();

            let vecs = {
                let fetched_content = fetch(url_str, c, false, true).await;
                let content = match fetched_content {
                    Ok(c) => c.as_str(),
                    Err(FetchError::StatusCode(status)) => {
                        fail_log(
                            format!(
                                "\x1b[1;91m{status}\x1b[0m \x1b[96m{}\x1b[0m {}",
                                Arc::strong_count(&sender) - 1,
                                url_str
                            ),
                            fail,
                            url,
                        );
                        decrement(sender);
                        return;
                    }
                    Err(FetchError::ReqwestError(err)) => {
                        fail_log(
                            format!(
                                "\x1b[1;91mError downloading\x1b[0m \x1b[96m{}\x1b[0m {}, {}",
                                Arc::strong_count(&sender) - 1,
                                url_str,
                                err
                            ),
                            fail,
                            url,
                        );
                        decrement(sender);
                        return;
                    }
                };
                download_wrapper(&url, dir, content.as_bytes(), true, fail).await;

                let doc = Html::parse_document(&content);
                let selector = Selector::parse("a[href]").unwrap();
                let img_selector = Selector::parse("img[src]").unwrap();
                let js_selector = Selector::parse("script[src]").unwrap();
                let css_selector = Selector::parse("link[rel=\"stylesheet\"]").unwrap();
                let element_groups = [
                    doc.select(&img_selector),
                    doc.select(&js_selector),
                    doc.select(&css_selector),
                    doc.select(&selector),
                ];

                let mut vecs = [vec![], vec![], vec![], vec![]];
                for i in 0..4 {
                    let link_attr = if i < 2 { "src" } else { "href" };
                    for element in element_groups[i].clone() {
                        if let Some(attr) = element.value().attr(link_attr) {
                            if let Ok(link) = url.join(attr) {
                                vecs[i].push(link)
                            }
                        }
                    }
                }
                vecs
            };

            for img_src in &vecs[0] {
                let src = img_src.to_string();
                if !urls.contains(&src) {
                    urls.insert(src);
                    if let Err(_) = imgs_sender.send(img_src.to_owned()).await {
                        eprintln!("Receiver dropped") //returns here.
                    }
                }
            }

            // &vecs[1..2] causes a segmentation fault
            for links in &vecs[1..3] {
                for link in links {
                    let l = link.to_string();
                    if !urls.contains(&l) {
                        urls.insert(l);
                        if let Err(_) = js_css_sender.send(link.to_owned()).await {
                            // TODO: remove this if-let cause the server is garanteed to not be dropped
                            eprintln!("Receiver dropped") //returns here.
                        }
                    }
                }
            }

            let mut sent = false;
            for link in &vecs[3] {
                let path = link.path();
                let link = if let Ok(joined_link) = link.join(path) {
                    joined_link
                } else {
                    continue;
                };
                let l = link.to_string();

                let contained = !urls.contains(&l);
                if l.starts_with(&*base_url) && contained {
                    urls.insert(l);
                    if let Err(_) = sender.send(link).await {
                        eprintln!("Receiver dropped") //returns here.
                    }
                    sent = true;
                }
            }

            println!(
                "\x1b[96m{}\x1b[0m {}",
                Arc::strong_count(&sender) - 1,
                url_str
            );

            if Arc::strong_count(&sender) == 2 && !sent {
                task::yield_now().await;
                decrement(sender);
            }
        });
    }
    set.join_all().await;

    drop(js_css_tx);
    drop(imgs_tx);

    download_resources(&mut js_css_rx, &client, &dir, false, duration, &f_js_css).await;
    download_resources(&mut imgs_rx, &client, &dir, true, duration, &f_imgs).await;

    let string_urls = stringify_urls(urls, String::new());
    let failed_urls = stringify_urls(failed, base_url.to_string() + "\n----\n");
    let failed_js_css_urls = stringify_urls(f_js_css, String::from("----js_css\n"));
    let failed_imgs_urls = stringify_urls(f_imgs, String::from("----imgs\n"));

    fs::write(
        dir.to_string() + "/visited-log-urls.bin",
        serialize(&string_urls).unwrap(),
    )
    .await
    .unwrap();
    fs::write(
        dir.to_string() + "/failed-log.bin",
        serialize(&(failed_urls + &failed_js_css_urls + &failed_imgs_urls)).unwrap(),
    )
    .await
    .unwrap();

    println!("\x1b[1;93mdone\x1b[0m");
}
