use std::env;
use std::error::Error;
use std::time::Duration;

use reqwest::blocking::Client;
use reqwest::header::{HeaderMap, HeaderValue, ACCEPT, ACCEPT_ENCODING, ACCEPT_LANGUAGE, CACHE_CONTROL, CONTENT_TYPE, ORIGIN, PRAGMA, REFERER, USER_AGENT};
use serde::{Deserialize, Serialize};
use serde_json::json;

const GRAPHQL_URL: &str = "https://api.propertyhub.in.th/graphql";
const DEFAULT_ZONE_ID: &str = "313";
const DEFAULT_LOCALE: &str = "TH";
const DEFAULT_MAX_PRICE: u32 = 10000;
const DEFAULT_PER_PAGE: u32 = 60;
const DEFAULT_ORDER: &str = "REFRESHED_AT";
const PERSISTED_QUERY_HASH: &str = "0583b83bf32bf57a9f58948244f716f1829de4f824e5fc7d75d550404dadaec8";

type DynError = Box<dyn Error + Send + Sync>;

#[derive(Debug, Deserialize)]
struct GraphQlResponse {
    data: GraphQlData,
}

#[derive(Debug, Deserialize)]
struct GraphQlData {
    #[serde(rename = "zoneListings")]
    zone_listings: ZoneListings,
}

#[derive(Debug, Deserialize)]
struct ZoneListings {
    status: String,
    error: Option<serde_json::Value>,
    pagination: Pagination,
    result: Vec<Listing>,
}

#[derive(Debug, Deserialize)]
struct Pagination {
    #[serde(rename = "page")]
    _page: u32,
    #[serde(rename = "perPage")]
    _per_page: u32,
    #[serde(rename = "totalCount")]
    total_count: u32,
    #[serde(rename = "totalPages")]
    total_pages: u32,
}

#[derive(Debug, Deserialize)]
struct Listing {
    id: String,
    title: String,
    slug: String,
    project: Project,
    location: Option<Location>,
    price: Price,
    #[serde(rename = "roomInformation")]
    room_information: RoomInformation,
    #[serde(default, rename = "createdAt")]
    created_at: Option<String>,
    #[serde(default, rename = "updatedAt")]
    updated_at: Option<String>,
    #[serde(default, rename = "refreshedAt")]
    refreshed_at: Option<String>,
}

#[derive(Debug, Deserialize)]
struct Project {
    #[serde(rename = "id")]
    _id: String,
    name: String,
    #[serde(rename = "nameEnglish")]
    name_english: Option<String>,
    address: Option<String>,
    slug: String,
    #[serde(rename = "listingCountByPostType")]
    listing_count_by_post_type: Option<ListingCountByPostType>,
}

#[derive(Debug, Deserialize)]
struct ListingCountByPostType {
    #[serde(rename = "FOR_RENT")]
    for_rent: Option<CountValue>,
    #[serde(rename = "FOR_SALE")]
    for_sale: Option<CountValue>,
}

#[derive(Debug, Deserialize)]
struct CountValue {
    #[serde(rename = "listingCount")]
    listing_count: Option<u32>,
}

#[derive(Debug, Deserialize, Serialize)]
struct Location {
    lat: f64,
    lng: f64,
}

#[derive(Debug, Deserialize)]
struct Price {
    #[serde(rename = "forRent")]
    for_rent: Option<ForRentPrice>,
}

#[derive(Debug, Deserialize)]
struct ForRentPrice {
    monthly: MonetaryValue,
}

#[derive(Debug, Deserialize)]
struct MonetaryValue {
    #[serde(default)]
    price: Option<f64>,
}

#[derive(Debug, Deserialize)]
struct RoomInformation {
    #[serde(default, rename = "numberOfBed")]
    number_of_bed: Option<u32>,
    #[serde(default, rename = "numberOfBath")]
    number_of_bath: Option<u32>,
    #[serde(default, rename = "roomArea")]
    room_area: Option<f64>,
    #[serde(default, rename = "onFloor")]
    on_floor: Option<String>,
    #[serde(default, rename = "roomType")]
    room_type: Option<String>,
}

#[derive(Debug, Serialize)]
struct ScrapedListing {
    id: String,
    title: String,
    url: String,
    project_name: String,
    project_name_english: Option<String>,
    project_slug: String,
    project_url: String,
    project_address: Option<String>,
    project_rent_count: Option<u32>,
    project_sale_count: Option<u32>,
    monthly_rent_thb: Option<f64>,
    bedrooms: Option<u32>,
    bathrooms: Option<u32>,
    room_area_m2: Option<f64>,
    floor: Option<String>,
    room_type: Option<String>,
    location: Option<Location>,
    created_at: Option<String>,
    updated_at: Option<String>,
    refreshed_at: Option<String>,
}

#[derive(Debug, Serialize)]
struct ScrapeResult {
    source: String,
    zone_id: String,
    locale: String,
    pages_scraped: u32,
    total_pages: u32,
    total_count: u32,
    listings: Vec<ScrapedListing>,
}

fn main() -> Result<(), DynError> {
    let config = Config::from_args()?;
    let client = Client::builder()
        .timeout(Duration::from_secs(60))
        .default_headers(default_headers(&config)?)
        .build()?;

    let first_page = fetch_zone_page(&client, &config, 1)?;
    let total_pages = first_page.pagination.total_pages;
    let total_count = first_page.pagination.total_count;
    let pages_to_scrape = if config.scrape_all {
        total_pages
    } else {
        config.max_pages.min(total_pages)
    };

    let mut listings = Vec::new();
    listings.extend(first_page.result.into_iter().map(to_scraped_listing));

    for page in 2..=pages_to_scrape {
        let response = fetch_zone_page(&client, &config, page)?;
        listings.extend(response.result.into_iter().map(to_scraped_listing));
    }

    let output = ScrapeResult {
        source: GRAPHQL_URL.to_string(),
        zone_id: config.zone_id,
        locale: config.locale,
        pages_scraped: pages_to_scrape,
        total_pages,
        total_count,
        listings,
    };

    println!("{}", serde_json::to_string_pretty(&output)?);
    Ok(())
}

struct Config {
    zone_id: String,
    locale: String,
    max_price: Option<u32>,
    per_page: u32,
    order: String,
    max_pages: u32,
    scrape_all: bool,
}

impl Config {
    fn from_args() -> Result<Self, DynError> {
        let mut zone_id = DEFAULT_ZONE_ID.to_string();
        let mut locale = DEFAULT_LOCALE.to_string();
        let mut max_price = Some(DEFAULT_MAX_PRICE);
        let mut per_page = DEFAULT_PER_PAGE;
        let mut order = DEFAULT_ORDER.to_string();
        let mut max_pages = 1u32;
        let mut scrape_all = false;

        let mut args = env::args().skip(1);
        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--zone-id" => zone_id = args.next().ok_or("--zone-id requires a value")?,
                "--locale" => locale = args.next().ok_or("--locale requires a value")?,
                "--max-price" => {
                    let value = args.next().ok_or("--max-price requires a value")?;
                    max_price = Some(value.parse()?);
                }
                "--no-max-price" => max_price = None,
                "--per-page" => {
                    let value = args.next().ok_or("--per-page requires a value")?;
                    per_page = value.parse()?;
                }
                "--order" => order = args.next().ok_or("--order requires a value")?,
                "--max-pages" => {
                    let value = args.next().ok_or("--max-pages requires a value")?;
                    max_pages = value.parse()?;
                }
                "--all" => scrape_all = true,
                "--help" | "-h" => {
                    print_help();
                    std::process::exit(0);
                }
                other => return Err(format!("unknown arg: {other}").into()),
            }
        }

        Ok(Self {
            zone_id,
            locale,
            max_price,
            per_page,
            order,
            max_pages,
            scrape_all,
        })
    }
}

fn print_help() {
    eprintln!(
        "Usage: property_scraping [--zone-id ID] [--locale TH] [--max-price N|--no-max-price] [--per-page N] [--order ORDER] [--max-pages N] [--all]"
    );
}

fn default_headers(config: &Config) -> Result<HeaderMap, DynError> {
    let mut headers = HeaderMap::new();
    headers.insert(
        USER_AGENT,
        HeaderValue::from_static("Mozilla/5.0 (X11; Ubuntu; Linux x86_64; rv:151.0) Gecko/20100101 Firefox/151.0"),
    );
    headers.insert(ACCEPT, HeaderValue::from_static("*/*"));
    headers.insert(ACCEPT_LANGUAGE, HeaderValue::from_static("en-US,en;q=0.9"));
    headers.insert(ACCEPT_ENCODING, HeaderValue::from_static("gzip, deflate, br, zstd"));
    headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
    headers.insert("locale", HeaderValue::from_str(&config.locale)?);
    headers.insert("version", HeaderValue::from_static("v3"));
    headers.insert(ORIGIN, HeaderValue::from_static("https://propertyhub.in.th"));
    headers.insert(REFERER, HeaderValue::from_static("https://propertyhub.in.th/"));
    headers.insert(PRAGMA, HeaderValue::from_static("no-cache"));
    headers.insert(CACHE_CONTROL, HeaderValue::from_static("no-cache"));
    Ok(headers)
}

fn fetch_zone_page(client: &Client, config: &Config, page: u32) -> Result<ZoneListings, DynError> {
    let body = json!({
        "operationName": "zoneListings",
        "variables": {
            "page": page,
            "perPage": config.per_page,
            "locale": config.locale,
            "order": config.order,
            "listingAttributes": {
                "zoneId": config.zone_id,
                "zoneIds": [],
                "postType": "FOR_RENT",
                "propertyType": "CONDO",
                "price": {
                    "min": null,
                    "max": config.max_price,
                }
            }
        },
        "extensions": {
            "persistedQuery": {
                "version": 1,
                "sha256Hash": PERSISTED_QUERY_HASH,
            }
        }
    });

    let response = client.post(GRAPHQL_URL).json(&body).send()?.error_for_status()?;
    let parsed: GraphQlResponse = response.json()?;
    let zone_listings = parsed.data.zone_listings;

    if zone_listings.status != "SUCCESS" {
        return Err(format!("GraphQL error: {:?}", zone_listings.error).into());
    }

    Ok(zone_listings)
}

fn to_scraped_listing(listing: Listing) -> ScrapedListing {
    let Project {
        _id: _,
        name,
        name_english,
        address,
        slug,
        listing_count_by_post_type,
    } = listing.project;

    let project_url = format!("https://propertyhub.in.th/en/projects/{}", slug);
    let detail_url = format!(
        "https://propertyhub.in.th/en/listings/{}---{}",
        listing.slug, listing.id
    );
    let project_counts = listing_count_by_post_type;

    ScrapedListing {
        id: listing.id,
        title: listing.title,
        url: detail_url,
        project_name: name,
        project_name_english: name_english,
        project_slug: slug,
        project_url,
        project_address: address,
        project_rent_count: project_counts
            .as_ref()
            .and_then(|counts| counts.for_rent.as_ref())
            .and_then(|count| count.listing_count),
        project_sale_count: project_counts
            .as_ref()
            .and_then(|counts| counts.for_sale.as_ref())
            .and_then(|count| count.listing_count),
        monthly_rent_thb: listing.price.for_rent.and_then(|rent| rent.monthly.price),
        bedrooms: listing.room_information.number_of_bed,
        bathrooms: listing.room_information.number_of_bath,
        room_area_m2: listing.room_information.room_area,
        floor: listing.room_information.on_floor,
        room_type: listing.room_information.room_type,
        location: listing.location,
        created_at: listing.created_at,
        updated_at: listing.updated_at,
        refreshed_at: listing.refreshed_at,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_zone_listings_response() {
        let body = r#"
        {
          "data": {
            "zoneListings": {
              "status": "SUCCESS",
              "error": null,
              "pagination": {
                "page": 1,
                "perPage": 2,
                "totalCount": 98,
                "totalPages": 49
              },
              "result": [
                {
                  "id": "2976350",
                  "title": "Example",
                  "slug": "example-listing",
                  "project": {
                    "id": "3163",
                    "name": "Project TH",
                    "nameEnglish": "Project EN",
                    "address": "Huai Khwang Bangkok",
                    "slug": "project-en",
                    "listingCountByPostType": {
                      "FOR_RENT": { "listingCount": 264 },
                      "FOR_SALE": { "listingCount": 142 }
                    }
                  },
                  "location": { "lat": 13.7, "lng": 100.5 },
                  "postType": "FOR_RENT",
                  "propertyType": "CONDO",
                  "price": { "forRent": { "monthly": { "type": "AMOUNT", "price": 10000 } } },
                  "roomInformation": { "numberOfBed": 1, "numberOfBath": 1, "roomArea": 24, "onFloor": "22", "roomType": "ONE_BED_ROOM" }
                }
              ]
            }
          }
        }
        "#;

        let parsed: GraphQlResponse = serde_json::from_str(body).expect("parse");
        let zone = parsed.data.zone_listings;
        assert_eq!(zone.pagination.total_pages, 49);
        assert_eq!(zone.result.len(), 1);

        let listing = to_scraped_listing(zone.result.into_iter().next().unwrap());
        assert_eq!(listing.project_rent_count, Some(264));
        assert_eq!(listing.project_sale_count, Some(142));
        assert_eq!(listing.monthly_rent_thb, Some(10000.0));
        assert_eq!(listing.floor.as_deref(), Some("22"));
    }
}
