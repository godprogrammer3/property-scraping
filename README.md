# property-scraping

Rust scraper for PropertyHub listing pages.

Target page:

`https://propertyhub.in.th/en/condo-for-rent/mrt-huai-khwang`

Run:

```bash
cargo run -- --url https://propertyhub.in.th/en/condo-for-rent/mrt-huai-khwang --max-pages 1
```

Fetch all pages:

```bash
cargo run -- --url https://propertyhub.in.th/en/condo-for-rent/mrt-huai-khwang --all
```

Output is JSON on stdout.
