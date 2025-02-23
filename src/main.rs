//NOTE: there is Url struct within reqwest crate
use std::{env::args, path::Path, sync::Arc, vec};

use dashmap::DashSet;
use reqwest::{Client, StatusCode};
use scraper::{Html, Selector};
use tokio::{
    fs,
    sync::{
        mpsc::{self, Receiver},
        Mutex,
    },
    task::{self, JoinSet},
};
use url::Url;

async fn download<T: AsRef<[u8]>>(url: &Url, dir: &str, content: T) {
    let the_fn = url.path();
    let path = Path::new(the_fn);
    if let Some(parent) = path.parent() {
        if parent.to_str() != Some("") {
            fs::create_dir_all(dir.to_owned() + parent.to_str().unwrap())
                .await
                .unwrap();
        }
    }
    if !the_fn.ends_with("/") {
        fs::write(dir.to_string() + the_fn, content).await.unwrap();
    }
}

async fn download_resources(
    rx: &mut Receiver<Url>,
    client: &Arc<Client>,
    dir: &'static str,
    bytes: bool,
) {
    let total = rx.len();
    //let progress = Arc::new(Mutex::new(0));
    let mut d = 0;
    // TODO: how to do template strings
    let text = if bytes {
        String::from("image") + if total == 1 { "" } else { "s" }
    } else {
        String::from("js & css files")
    };
    println!("\x1b[1;93mdownloading {total} {text}\x1b[0m");
    let mut set = JoinSet::new();
    while let Some(url) = rx.recv().await {
        let c = client.clone();
        //let p = progress.clone();
        d += 1;
        set.spawn(async move {
            //let mut d = p.lock().await;
            //*d += 1;
            let url_str = url.as_str();
            let res = c.get(url_str).send().await.unwrap();
            let status = res.status();
            if status != StatusCode::OK {
                println!(
                    "\x1b[1;91m{}\x1b[0m \x1b[96m{}/{}\x1b[0m {}",
                    status, d, total, url_str
                );
                return;
            } else {
                println!("\x1b[96m{}/{}\x1b[0m {}", d, total, url_str);
            }
            let content = if bytes {
                (&*res.bytes().await.unwrap()).to_vec()
            } else {
                let dom = res.text().await.unwrap();
                dom.into_bytes()
            };
            download(&url, dir, content).await;
        });
    }
    set.join_all().await;
}

#[tokio::main]
async fn main() {
    let scraper = vec![
        (
            "https://doc.rust-lang.org/std/",
            "section#main-content, nav.sidebar",
            "dist-new-test10-new",
            1,
        ),
        (
            "https://www.w3schools.com/js/default.asp",
            "*",
            "dist-w3-js",
            1,
        ),
        (
            "https://www.w3schools.com/python/default.asp",
            "*",
            "dist-w3-python",
            1,
        ),
        (
            "http://localhost:7070/url/latest/url/",
            "*",
            "dist_test_base",
            4,
        ),
        (
            "https://docs.rs/bincode/latest/bincode/all.html",
            "*",
            "dist-bincode_5",
            3,
        ),
        (
            "https://docs.rs/reqwest/latest/reqwest/",
            "*",
            "dist-reqwest2",
            3,
        ),
        (
            "https://docs.rs/scraper/latest/scraper/",
            "*",
            "dist-scraper2",
            3,
        ),
        ("https://docs.rs/tokio/latest/tokio/", "*", "dist-tokio2", 3),
        (
            "https://doc.rust-lang.org/rust-by-example/index.html",
            "*",
            "dist-rust-by-example2",
            1,
        ),
        (
            "https://docs.rs/tokio-stream/0.1.16/tokio_stream/index.html",
            "*",
            "dist-tokio-stream",
            3,
        ),
        (
            "https://docs.rs/futures/latest/futures/",
            "*",
            "dist-futures",
            3,
        ),
        ("https://docs.rs/url/latest/url/", "*", "dist-url", 3),
        (
            "https://docs.rs/dashmap/latest/dashmap/",
            "*",
            "dist-dashmap",
            3,
        ),
        (
            "https://docs.rs/scc/latest/scc/all.html",
            "*",
            "dist-scc2",
            3,
        ),
        (
            "https://doc.rust-lang.org/cargo/guide/",
            "*",
            "dist-cargo-guide",
            2,
        ),
        (
            "https://doc.rust-lang.org/nightly/clippy/",
            "*",
            "dist-clippy2",
            2,
        ),
        ("https://doc.rust-lang.org/book/", "*", "dist-book", 1),
        (
            "https://doc.rust-lang.org/stable/reference/",
            "*",
            "dist-rust-reference2",
            2,
        ),
        (
            "https://rust-lang.github.io/api-guidelines/",
            "*",
            "dist-api-guidelines",
            1,
        ),
        (
            "https://rust-lang-nursery.github.io/rust-cookbook/web/clients.html",
            "*",
            "dist-rust-cookbook",
            2,
        ),
        (
            "https://rust-lang.github.io/async-book/",
            "*",
            "dist-rust-async-recent",
            1,
        ),
        (
            "https://docs.aiohttp.org/en/stable/",
            "*",
            "dist-aiohttp-test",
            2,
        ),
        ("http://localhost:8080", "*", "dist-local-host", 0),
        ("http://localhost:7070", "*", "dist-local-host", 3),
        (
            "http://127.0.0.1:8080/ccc/more/more.html",
            "*",
            "dist-local-host",
            3,
        ),
        (
            "http://localhost:7070/url/latest/url/struct.Url.html",
            "*",
            "dist-local",
            3,
        ),
        (
            "http://localhost:8080/bincode/latest/bincode/index.html",
            "*",
            "dist-local-bincode",
            3,
        ),
        (
            "http://localhost:8080/tokio/latest/tokio/index.html",
            "*",
            "dist-local-tokio",
            3,
        ),
        (
            "http://localhost:8080/reqwest/latest/reqwest/index.html",
            "*",
            "dist-local-reqwest",
            3,
        ),
        (
            "http://localhost:8080/scraper/latest/scraper/index.html",
            "*",
            "dist-local-scraper",
            3,
        ),
        (
            "http://localhost:8080/tokio-stream/0.1.16/tokio_stream/index.html",
            "*",
            "dist-local-tokio-stream",
            3,
        ),
        (
            "http://localhost:7070/futures/latest/futures/index.html",
            "*",
            "dist-local-futures",
            3,
        ),
        (
            "http://localhost:8080/dashmap/latest/dashmap/index.html",
            "*",
            "dist-local-dashmap",
            3,
        ),
        (
            "http://localhost:8080/scc/latest/scc/index.html",
            "*",
            "dist-local-scc",
            3,
        ),
    ];

    let (tx, mut rx) = mpsc::channel(1000);
    let (imgs_tx, mut imgs_rx) = mpsc::channel(1000);
    let (js_css_tx, mut js_css_rx) = mpsc::channel(1000);
    let tx = Arc::new(tx);
    let imgs_tx = Arc::new(imgs_tx);
    let js_css_tx = Arc::new(js_css_tx);
    let client = Arc::new(Client::new());
    let urls = Arc::new(DashSet::new());

    // till 191919191919191919191919191919191919191919191919191919191919191919191919191919191919
    let sites_index = 2;
    //let sites_index = 20;
    // let sites_index = 23;
    // let sites_index = 29;
    let url = scraper[sites_index].0;
    let dir = scraper[sites_index].2;
    let base_index = scraper[sites_index].3;

    // let arguments: Vec<String> = args().collect();
    // // println!("~~> {:?},{}", arguments, arguments.len());
    // let args = match arguments.len() {
    //     4 => {
    //         let u = arguments[1].to_owned();
    //         let s = arguments[2].to_owned();
    //         // let d = arguments[3].to_owned();
    //         let b = arguments[3].to_owned();
    //         [u, s, b]
    //     }
    //     _ => {
    //         panic!("Usage: {} url selector dist", arguments[0])
    //     }
    // };
    // let url = args[0].as_str();
    // let tags = args[1].as_str();
    // // let dir = args[2].as_str();
    // let base_index = args[2].parse().unwrap();

    let url = Url::parse(url).unwrap();

    if let Ok(false) = fs::try_exists(dir).await {
        fs::create_dir(dir).await.unwrap();
    }

    let p = Path::new(url.path()).iter().take(base_index + 1);
    if p.clone().count() <= base_index {
        panic!("url path is less than base {}", base_index);
    }

    let base_path = Arc::new(p.fold("".to_string(), |acc, e| {
        let slash = if e == "/" || acc == "/" { "" } else { "/" };
        acc + slash + e.to_str().unwrap()
    }));

    // NOTE: can I make this one work without the need for Arc
    let base_url = url.join(&base_path).unwrap().to_string();
    let base_url = Arc::new(base_url);

    urls.insert(url.to_string());
    if let Err(_) = tx.clone().send(url).await {
        println!("Receiver dropped");
        return;
    }

    let mut set = JoinSet::new();
    while let Some(url) = rx.recv().await {
        let c = client.clone();
        let sender = Arc::clone(&tx);
        let imgs_sender = Arc::clone(&imgs_tx);
        let js_css_sender = Arc::clone(&js_css_tx);
        let urls = Arc::clone(&urls);
        let base_url = Arc::clone(&base_url);

        set.spawn(async move {
            let url_str = url.as_str();

            let vecs = {
                let res = c.get(url_str).send().await.unwrap();
                let status = res.status();
                if status != StatusCode::OK {
                    println!(
                        "\x1b[1;91m{status}\x1b[0m \x1b[96m{}\x1b[0m {}",
                        // NOTE: should I show this number, isn't it supposed to wait until after
                        // the links have been sent. Or is it fine just like this??
                        Arc::strong_count(&sender) - 1,
                        url_str
                    );
                    let ptr = Arc::into_raw(sender);
                    unsafe {
                        Arc::decrement_strong_count(ptr);
                    }
                    return;
                }

                let content = res.text().await.unwrap();
                download(&url, dir, content.as_bytes()).await;

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
                        let link = url.join(element.value().attr(link_attr).unwrap()).unwrap();
                        vecs[i].push(link);
                    }
                }
                vecs
            };

            for img_src in &vecs[0] {
                let src = img_src.to_string();
                if !urls.contains(&src) {
                    urls.insert(src);
                    if let Err(_) = imgs_sender.send(img_src.to_owned()).await {
                        println!("Receiver dropped");
                        return;
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
                            println!("Receiver dropped");
                            return;
                        }
                    }
                }
            }

            let mut sent = false;
            for link in &vecs[3] {
                let path = link.path();
                println!("{:?} {:?}", link, path);

                let link = link.join(path).unwrap();
                let l = link.to_string();

                let contained = !urls.contains(&l);
                if l.starts_with(&*base_url) && contained {
                    urls.insert(l);
                    if let Err(_) = sender.send(link).await {
                        println!("Receiver dropped");
                        return;
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
                let ptr = Arc::into_raw(sender);
                unsafe {
                    Arc::decrement_strong_count(ptr);
                    Arc::decrement_strong_count(ptr);
                }
            }
        });
    }
    set.join_all().await;

    drop(js_css_tx);
    drop(imgs_tx);

    download_resources(&mut js_css_rx, &client, dir, false).await;
    download_resources(&mut imgs_rx, &client, dir, true).await;

    println!("\x1b[1;93mdone\x1b[0m");
}
