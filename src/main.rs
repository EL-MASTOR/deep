use core::{panic, result::Result};
use dashmap::DashSet;
use reqwest::{Client, Error, StatusCode};
use scraper::{Html, Selector};
use std::{
    env::args, io::Result as io_result, path::Path, process::exit, sync::Arc, time::Duration, vec,
};
use tokio::{
    fs::{create_dir, create_dir_all, read_to_string, try_exists, write},
    sync::mpsc::{channel, Receiver, Sender},
    task::{yield_now, JoinSet},
    time::sleep,
};
use url::Url;

async fn download<T: AsRef<[u8]>>(
    url: &Url,
    dir: Arc<String>,
    content: T,
    is_html: bool,
) -> io_result<()> {
    let the_fn = url.path();
    let path = Path::new(the_fn);
    let dir = dir.to_string();
    let mut tail_dir = String::from("/");
    let mut final_path = dir.to_owned() + the_fn;
    if is_html && !the_fn.ends_with(".html") {
        tail_dir = tail_dir + path.file_name().unwrap().to_str().unwrap();
        final_path = final_path + "/index.html";
    }
    if let Some(parent) = path.parent() {
        if parent.to_str() != Some("") {
            create_dir_all(dir + parent.to_str().unwrap() + &tail_dir).await?;
        }
    }
    write(final_path, content).await?;
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
    StringContent((bool, String)),
    VectContent(Vec<u8>),
}

impl From<Error> for FetchError {
    fn from(err: Error) -> FetchError {
        FetchError::ReqwestError(err)
    }
}

impl FetchedContent {
    fn text_string(self) -> String {
        match self {
            Self::StringContent(s) => s.1,
            Self::VectContent(_) => {
                panic!("Called as_str() on an FetchedContent::VectContent variant")
            }
        }
    }
    fn is_html(&self) -> bool {
        match self {
            Self::StringContent(s) => s.0,
            Self::VectContent(_) => false,
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
    let content_type = res.headers().get("content-type").unwrap();
    let is_html = if content_type == "text/html" {
        true
    } else {
        false
    };
    let content = if bytes {
        (&*res.bytes().await?).to_vec()
    } else {
        let dom = res.text().await?;
        if pages {
            return Ok(FetchedContent::StringContent((is_html, dom)));
        }
        dom.into_bytes()
    };
    Ok(FetchedContent::VectContent(content))
}

async fn download_wrapper<T: AsRef<[u8]>>(
    url: &Url,
    dir: Arc<String>,
    content: T,
    is_html: bool,
    fail_safe: Arc<DashSet<String>>,
) {
    if let Err(err) = download(&url, dir, content, is_html).await {
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
                        download_wrapper(&url, dir, value.1, value.0, fail).await
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

async fn read_file(log_file: &str) -> String {
    if let Ok(log) = read_to_string(Path::new(log_file)).await {
        log
    } else {
        exit_code_1(format!(
            "Error occured while attempting to read ./{}. Does this file exist?",
            log_file
        ))
    }
}

#[tokio::main]
async fn main() {
    // TODO: try unbounded_channel
    let (tx, mut rx) = channel(10000000);
    let (imgs_tx, mut imgs_rx) = channel(10000000); // TODO: warn about this in this limit in the
                                                    // readme. But check if there's any
                                                    // downsides for encreasing this value
    let (js_css_tx, mut js_css_rx) = channel(10000000);
    let tx = Arc::new(tx);
    let imgs_tx = Arc::new(imgs_tx);
    let js_css_tx = Arc::new(js_css_tx);
    let client = Arc::new(Client::new());
    let urls = Arc::new(DashSet::new());
    let failed = Arc::new(DashSet::new());
    let f_imgs = Arc::new(DashSet::new());
    let f_js_css = Arc::new(DashSet::new());
    let ignore: Arc<Vec<String>>;

    let mut arguments: Vec<String> = args().collect();
    let retrying = arguments.len() == 3 && arguments[1] == "-a"; //the number of arguments supplied should
                                                                 //not exceed 2 elements. Otherwise
                                                                 //this wouldn't work.
    let duration = if (arguments.len() >= 5 && arguments[4] != "-i") || retrying {
        let idx = if retrying { 2 } else { 4 };
        let delay = arguments.remove(idx);
        if let Ok(d) = delay.parse() {
            d
        } else {
            exit_code_1(format!(
                "'{}' is invalid argument. `FREQ` should be a number",
                delay
            ));
        }
    } else {
        0
    };
    let [dir, base_url] = if arguments.len() == 2 && arguments[1] == "-a" {
        let mut to_ignore = Vec::<String>::new();
        let missed_urls = read_file("_deep-logs/failsafe.log").await;
        let visited_urls = read_file("_deep-logs/visited.log").await;
        for v in visited_urls.lines() {
            urls.insert(v.to_string());
        }
        let mut categories = missed_urls.split("----");
        let base_url = Arc::new({ &categories.next().unwrap().trim_end()[6..] }.to_owned());
        let mut ignored_links = categories.next().unwrap().lines();
        ignored_links.next().unwrap();
        for ignored in ignored_links {
            to_ignore.push(ignored.to_string());
        }
        ignore = Arc::new(to_ignore);
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
                the_tx.send(Url::parse(url).unwrap()).await.unwrap();
            }
        }
        [Arc::new(String::from(".")), base_url]
    } else {
        let mut to_ignore = Vec::new();
        let usage = format!(
            "Usage:\n\t\x1b[1m\x1b[92m{}\x1b[39m URL DIR BASE [FREQ] [-i IGNORED]\x1b[0m\nor\n\t\x1b[1m\x1b[92m{}\x1b[39m -a [FREQ]\x1b[0m",
            arguments[0], arguments[0]
        );
        let args = match arguments.len() {
            4.. => {
                let d = arguments.remove(3);
                let b = arguments.remove(2);
                let u = arguments.remove(1);
                if arguments.len() > 2 {
                    if arguments[1] == "-i" {
                        arguments.remove(1);
                        to_ignore = arguments;
                        to_ignore.remove(0);
                    } else {
                        exit_code_1(usage);
                    }
                }
                [u, b, d]
            }
            _ => {
                exit_code_1(usage);
            }
        };

        let [url, dir, base] = args;

        let dir = Arc::new(dir);

        let base_index = if let Ok(bi) = base.parse() {
            bi
        } else {
            exit_code_1(format!(
                "'{}' is invalid argument, `BASE` should be a number",
                base
            ));
        };

        let url = if let Ok(url) = Url::parse(&url) {
            url
        } else {
            exit_code_1(format!("invalid url"));
        };

        let d = &*dir;
        if let Ok(false) = try_exists(d).await {
            if let Err(err) = create_dir(d).await {
                exit_code_1(format!("err creating {d}: {err}"));
            };
        }

        let p = Path::new(url.path()).iter().take(base_index + 1);
        let p_count = p.clone().count();
        if p_count <= base_index {
            exit_code_1(format!(
                "url path components count is less than BASE: {} <= {}\ntry decreasing BASE",
                p_count, base_index
            ));
        }

        let base_path = p.fold("".to_string(), |acc, e| {
            let slash = if e == "/" || acc == "/" { "" } else { "/" };
            acc + slash + e.to_str().unwrap()
        }) + if base_index == 0 { "" } else { "/" };
        let base_url = url.join(&base_path).unwrap().to_string();
        let base_url = Arc::new(base_url);

        for ignored in to_ignore.iter_mut() {
            *ignored = base_url.to_string() + ignored;
        }
        ignore = Arc::new(to_ignore);

        urls.insert(url.to_string());
        tx.send(url).await.unwrap();
        [dir, base_url]
    };

    if rx.is_empty() {
        rx.close();
    }
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
        let ignored = Arc::clone(&ignore);

        if duration != 0 {
            sleep(Duration::from_millis(duration)).await;
        }

        set.spawn(async move {
            let url_str = url.as_str();

            let vecs = {
                let fetched_content = fetch(url_str, c, false, true).await;
                let is_html;
                let content = match fetched_content {
                    Ok(c) => {
                        is_html = c.is_html();
                        c.text_string()
                    }
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
                download_wrapper(&url, dir, content.as_bytes(), is_html, fail).await;

                let doc = Html::parse_document(&content);
                let selector = Selector::parse("a[href]").unwrap();
                let img_selector = Selector::parse("img[src]").unwrap();
                let js_selector = Selector::parse("script[src]").unwrap();
                let css_selector = Selector::parse("link[rel=\"stylesheet\"]").unwrap();
                let mut element_groups = [
                    doc.select(&img_selector),
                    doc.select(&js_selector),
                    doc.select(&css_selector),
                    doc.select(&selector),
                ];

                let mut vecs = [vec![], vec![], vec![], vec![]];
                for i in 0..4 {
                    let link_attr = if i < 2 { "src" } else { "href" };
                    // NOTE: I made this &mut, so we don't clone it.
                    for element in &mut element_groups[i] {
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
                    imgs_sender.send(img_src.to_owned()).await.unwrap();
                }
            }

            // &vecs[1..2] causes a segmentation fault
            for links in &vecs[1..3] {
                for link in links {
                    let l = link.to_string();
                    if !urls.contains(&l) {
                        urls.insert(l);
                        js_css_sender.send(link.to_owned()).await.unwrap();
                    }
                }
            }

            let mut sent = false;
            'outer: for link in &vecs[3] {
                let path = link.path();
                let link = if let Ok(joined_link) = link.join(path) {
                    joined_link
                } else {
                    continue;
                };
                let l = link.to_string();

                let contained = urls.contains(&l);
                if l.starts_with(&*base_url) && !contained {
                    // PERF: urls contains unvisited `ignored` matched urls urls also. So this for loop isn't repeated.
                    urls.insert(l.clone());
                    for item in &*ignored {
                        if l.starts_with(item) {
                            continue 'outer;
                        }
                    }
                    sender.send(link).await.unwrap();
                    sent = true;
                }
            }

            println!(
                "\x1b[96m{}\x1b[0m {}",
                Arc::strong_count(&sender) - 1,
                url_str
            );

            if Arc::strong_count(&sender) == 2 && !sent {
                yield_now().await;
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
    let failed_urls = stringify_urls(
        failed,
        String::from("BASE: ")
            + &base_url
            + "\n----ignored\n"
            + &ignore
                .iter()
                .fold(String::new(), |acc, x| acc + x.as_str() + "\n")
            + "----failed\n",
    );
    let failed_js_css_urls = stringify_urls(f_js_css, String::from("----js_css\n"));
    let failed_imgs_urls = stringify_urls(f_imgs, String::from("----imgs\n"));

    let deep_logs = dir.to_string() + "/_deep-logs";
    if let Ok(false) = try_exists(&deep_logs).await {
        create_dir(&deep_logs).await.unwrap();
    }
    write(deep_logs.to_owned() + "/visited.log", string_urls)
        .await
        .unwrap();
    write(
        deep_logs + "/failsafe.log",
        failed_urls + &failed_js_css_urls + &failed_imgs_urls,
    )
    .await
    .unwrap();

    println!("\x1b[1;93mdone\x1b[0m");
}
