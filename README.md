# property-scraping

Rust scraper for PropertyHub condo-for-rent listings.

Default target:

`https://propertyhub.in.th/en/condo-for-rent/mrt-huai-khwang`

Run:

```bash
cargo run -- --zone-id 313 --locale TH --max-price 10000 --per-page 60 --max-pages 1
```

Fetch all pages:

```bash
cargo run -- --all
```

CSV output:

```bash
cargo run -- --format csv
```

Disable project-page enrichment:

```bash
cargo run -- --no-project-details
```

Output is JSON by default. Each listing includes GraphQL listing data plus project-page detail fields.
