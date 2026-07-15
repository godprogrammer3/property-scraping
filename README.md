# property-scraping

Rust scraper for PropertyHub condo-for-rent listings.

Default preset:

`NearHuaikhwang`

Other presets:

- `NearSuttisan`
- `NearMrtRatchadaPisak`
- `NearLadPhrao`
- `ALL`

Run default scrape:

```bash
cargo run -- --near-mrt NearHuaikhwang --locale TH --max-price 10000 --per-page 60 --max-pages 1
```

Fetch all preset pages and merge results:

```bash
cargo run -- --near-mrt ALL --all
```

CSV output:

```bash
cargo run -- --near-mrt ALL --format csv
```

Disable project-page enrichment:

```bash
cargo run -- --no-project-details
```

Walk distance uses OSRM. Default base URL:

`https://router.project-osrm.org`

Override it with:

```bash
OSRM_BASE_URL=https://your-osrm.example cargo run -- --near-mrt NearSuttisan
```

JSON is default output. CSV columns:

`Id,Name,Link,Built,Rent Price,Near MRT,Distance(m),Area(m*m),Fl,Total Fl,Direction,Parking,Note`

Each listing includes GraphQL listing data plus project-page detail fields.
