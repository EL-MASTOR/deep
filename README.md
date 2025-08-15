I personally use it to download documentations for offline access, but it can be used with most websites out there.

# Installation
##### Straight forward installation
`cargo install --git https://github.com/EL-MASTOR/deep`
##### Build from source
`git clone https://github.com/EL-MASTOR/deep && cd https://github.com/EL-MASTOR/deep && cargo build --release`

# Usage
**The order is important!**
`deep URL DIR BASE [FREQ] [-i IGNORED]`
or `deep -a [FREQ]` for retrying failed URLs<a href="#3">^3</a>.

##### Quick try
You can try a quick example with:<br/>
`deep https://doc.rust-lang.org/nightly/clippy/ clippy 2`<br/>
You can see the files downloaded with `eza --icons --tree` or any tree listing program you have.<br/>
You can view a local version of the website by either:
* Going to `file:///path/to/clippy/nightly/clippy/index.html` in a chromium based browser.
* Or by `cd dist-clippy && live-server .` this will serve the files at port 8080, then you can go to `http://localhost:8080/nightly/clippy/`

# Explanation of how it works
The program uses an asynchronous `mpsc` channel that receives URLs and does some work to each URL. This works like a queue.<br/>
The program first send the URL that you provided to the channel, which takes that URL, and downloads its webpage at path<a href="#2">^2</a>, and then all the new `a` tag links in that page that met a certain criteria<a href="#1">^1</a> get sent to the same channel.<br/>
This is repeated to each new link until no new links are found.<br/>
That's why it is called `deep`, because it deeply dives into the website's tree to find new links to download.<br/>
Once all the web pages have been downloaded, it proceeds to download the js, css, and image links found in those pages.<br/>
You can view it in the browser as explained in the [Quick try](#quick-try) section above.

# Keynotes and usage tips
- The program is asynchronous and concurrent for most of its work.
- <span id="1">^1. </span> The criteria for picking URL links:<br/>
  ..- **The URL link should start with a base.** All the URL links found in those pages that don't start with this base are ignored.<br/>
  The base is determined by taking `BASE` argument you provided (which is a number) and picking up until those number of pathnames in the `URL` to be the base.<br/>
  So if the `URL` you provided is `https://example.com/a/b/c/d`, (The pathname here is `/a/b/c/d`), and you specified `BASE` to be 2, the base URL will be `https://example.com/a/b`. And if `BASE` is 0, the base path is `https://example.com`.<br/>
  The js, css and image links are exceptional, as they only get checked if the URL starts with the host, the pathname isn't included. So they get checked if they start with `https://example.com`, as in `https://example.com/script.js` even if `BASE` is 2.<br/>
  This also means that external scripts, styles and images that are not related to the website aren't included.<br/>
  You can't pick `BASE` to be more than the number of components in the pathname of the `URL` you provided. In the previous example, `/a/b/c/d` has only 4 components, so you can't pick `BASE` to be 5.<br/>
  ..- **The URLs are new.** Each URL, after they passed the base check, they get checked if they are new, to avoid repeating the work.<br/>
  The program stores each new URL in a concurrent hashset with O(1) search time, so when ever a new URL is found, it checks if it's already in the hashset (processed) or not (not yet processed). If not already present in the hashset, it gets sent to the channel to be processed and downloaded.<br/>
  Only the origin of the URLs are checked, meaning if a URL is `https://example.com/a/b/x/y?query=string#hash`, the `?query=string#hash` part is removed so only the origin is remained, which contains the host and the pathname `https://example.com/a/b/x/y`. This makes the program more efficient, so it does not include URLs that point to the same website but look differently.
- The optional argument `[FREQ]` (_frequency_) represents the amount of time in milliseconds between each request send.<br/>
  So, if you set `[FREQ]` to 10, it will only allow for sending a 100 requests per second.<br/>
  By default, the program sends as many requests as possible, depending on your connection speed.<br/>
  This regulation is helpful to use `deep` on the websites that will block you if you send requests fast.
- An optional argument `[-i IGNORED]` can be specified to ignore certain links.<br/>
  Here!s an example to illustrate:<br/>
  `deep https://example.com/a/b dest 1 -i c/ d/e/`<br/>
  The ignored URLs are formed like: base_url + ignored_argument<br/>
  Here, the base URL is `https://example.com/a/`.<br/>
  From this the ignored URLs will be: `https://example.com/a/c/`, `https://example.com/a/d/e/`.<br/>
  Any link that starts with any of these ignored URLs, will be ignored, and won't be downloaded.<br/>
  There are many reasons where you might want to ignore some links.<br/>
  > [!IMPORTANT]<br/>
  > The slash at the end ends the pathname. Whether it's present or not, might affect the amount of pages downloaded. Take a look at this example to better understand:<br/>
  > `deep https://example.com/x/y dest 1 -i a/`<br/>
  > The ignored link will be `https://example.com/x/a/`<br/>
  > `deep https://example.com/x/y dest 1 -i a`<br/>
  > The ignored link will be `https://example.com/x/a`<br/>
  > Did you notice the difference? The 2nd example without an ending `/` ignores `https://example.com/x/api` and `https://example.com/x/administration/x`, whereas the 1st one doesn't.<br/>
  > It is a good practice to always end your ignored pathnames with `/` to avoid ignoring links that you didn't intend to ignore. Unless, you want to ignore them too.<br/>
  You might specify as many ignored pathnames as you like.
- <span id="2">^2. </span> The path and filename of the downloaded file is determined from the pathname of the URL. Very straight forward. The page at `https://example.com/a/b/z.html` is downloaded to `DIR/a/b/z.html`. And the page at `https://example.com/a/b/y` is downloaded to `DIR/a/b/y/index.html` if it's an html file that doesn't end with ".html", otherwise it is just downloaded as is. `DIR` here is the directory name you provided. All messing directories are created.
- <span id="3">^3. </span> Did some pages failed to download? Worry not! You don't have to restart. All you have to do is cd into your `DIR` where you have downloaded the pages, then run `deep -a`. This will retry downloading the failed URLs.<br/>
  This uses the information stored in `_deep-logs/failsafe.log` and `_deep-logs/visited.log`.
  > [!IMPORTANT]<br/>
  > `_deep-logs/visited.log` contains both failed and ignored URLs if any.<br/>
  > if you wish to get the URLs downloaded in your computer, you can use either of these methods:<br/>
  >  - **From file paths:** File names and paths get determined from their URLs.<br/>
  > The root of `DIR` is the root of the websites, you just need to prefix it the domain name of the website.<br/>
  > It is important to note that URLs that don't end with ".html" get downloaded to "index.html".<br/>
  > "`DIR`/a/b/c.html" -> `https://example.com/a/b/c.html`<br/>
  > "`DIR`/x/y/z/index.html" -> `https://example.com/x/y/z`<br/>
  >  - **From log files:** You need to remove the failed and ignored URLs from `_deep-logs/visited.log`. You can get needed information needed from `_deep-logs/failsafe.log`. The latter is sectioned by `----`.<br/>
  > The 1st line is the base URL. From the 2nd line which starts with `----` until the next one, there are ignored URLs if any. You need to ignore any line in `_deep-logs/visited.log` that starts with any of these.<br/>
  > The other sections are failed URLs.

# Things to consider \[⚠️ IMPORTANT!!!\]
- Be careful of how you specify `URL`! A trailing `/` can make a huge difference if it's present or not.<br/>
  Make sure it is present in URLs that don't end with a filename, and absent on the ones that do.<br/>
  Examples of URLs that ends with a filename:<br/>
  `https://example/a/b.html`,<br/>
  `https://example/a/c.json`.<br/>
  Examples of URLs that don't ends with a filename:<br/>
  `https://example/a/x/`,<br/>
  `https://example/a/images/`.<br/>
  If you're not sure about whether to add a trailing `/` or not, just load the URL in the browser and copy the link from the browser's address bar. The browser will do the job for you figuring out whether to add `/` or not.

- Be careful picking `BASE` value. The lower `BASE` is, the more websites are downloaded. So choose it to be as high as you need it to be.<br/>
  Let's say you want to download the python documentation at `https://courseswebsite.com/python/default.asp`. <br/>
  Here you should make the `BASE` to be as high as you want it to be, in this case it will be 1 to download all the sub-urls of `https://courseswebsite.com/python`.<br/>
  If you lower `BASE` by 1, you will download all the sub-urls of `https://courseswebsite.com/`, which includes other courses and other things, when we only want the python course.<br/>
  So you should choose it as high as you want it to be.<br/>
  Do not set it to 3, since "default.asp" is just a single page and does not have any sub-urls, and `https://courseswebsite.com/python` is the root of the python course web pages.

- JavaScript does not get executed. Therefore content that loads dynamically won't be loaded.<br/>
Keep that in mind, if something is wrong about the downloaded pages, check if the content you want to download is statically loaded with `curl -o test.html URL`, then open "test.html" to see if the content you want gets loaded or not.

- If `[FREQ]` is not specified, the program doesn't put any restrictions upon sending requests.<br/>
  The number of requests you send is only affected by your connection speed.<br/>
  So, you might get IP-banned if some servers noticed that you send too many requests.<br/>
  Though, it is not very common. I have run into this situation only once with one website. Later, I set `[FREQ]` to 10, and it worked fine with the website.

# Some of the websites I've used `deep` on

```sh
deep https://doc.rust-lang.org/std/ dist-std 1
```
```sh
deep https://docs.rs/scraper/latest/scraper/ dist-scraper2 3
```
```sh
deep https://rust-lang-nursery.github.io/rust-cookbook/web/clients.html dist-rust-cookbook 2
```
```sh
deep https://developer.mozilla.org/en-US/docs/Web dist-mozilla 2
```
```sh
deep https://www.w3schools.com/js/default.asp dist-w3-js 1 10 # I had to send a request each 10ms, because the websites blocks IPs that flood it with requests. This is the only website I have encountered that does that. I didn't test it with a value lower than 10 though, so it might still work faster with lower values.
```
```sh
deep https://shopify.dev/docs dist-shopify 1 -i api/admin-graphql api/storefront # I had to ignore certain huge links because the website is huge
```
