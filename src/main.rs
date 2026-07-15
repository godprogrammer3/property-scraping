use std::env;
use std::error::Error;
use std::fmt;

use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};
use url::Url;

const DEFAULT_URL: &str = "https://propertyhub.in.th/en/condo-for-rent/mrt-huai-khwang";

#[derive(Debug, Deserialize)]
struct NextData {
    props: Props,
}

#[derive(Debug, Deserialize)]
struct Props {
    #[serde(rename = "pageProps")]
    page_props: PageProps,
}

#[derive(Debug, Deserialize)]
struct PageProps {
    listings: Vec<Listing>,
    pagination: Pagination,
}

#[derive(Debug, Deserialize)]
struct Listing {
    id: String,
    title: String,
    slug: String,
    project: Project,
    price: Price,
    #[serde(rename = "roomInformation")]
    room_information: RoomInformation,
    #[serde(default, rename = "updatedAt")]
    updated_at: Option<String>,
}

#[derive(Debug, Deserialize)]
struct Project {
    name: String,
    slug: String,
    #[serde(default)]
    address: Option<String>,
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

#[derive(Debug, Deserialize)]
struct Pagination {
    page: u32,
    #[serde(rename = "perPage")]
    per_page: u32,
    #[serde(rename = "totalCount")]
    total_count: u32,
    #[serde(rename = "totalPages")]
    total_pages: u32,
}

#[derive(Debug, Serialize)]
struct ScrapedListing {
    id: String,
    title: String,
    url: String,
    project_name: String,
    project_slug: String,
    project_address: Option<String>,
    monthly_rent_thb: Option<f64>,
    bedrooms: Option<u32>,
    bathrooms: Option<u32>,
    room_area_m2: Option<f64>,
    floor: Option<String>,
    room_type: Option<String>,
    updated_at: Option<String>,
}

#[derive(Debug, Serialize)]
struct ScrapeResult {
    source_url: String,
    pages_scraped: u32,
    total_pages: u32,
    total_count: u32,
    listings: Vec<ScrapedListing>,
}

#[derive(Debug)]
enum ScrapeError {
    MissingNextData,
    UnexpectedUrl(String),
}

impl fmt::Display for ScrapeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingNextData => write!(f, "could not find __NEXT_DATA__ in page HTML"),
            Self::UnexpectedUrl(url) => write!(f, "could not parse URL: {url}"),
        }
    }
}

impl Error for ScrapeError {}

fn main() -> Result<(), Box<dyn Error>> {
    let config = Config::from_args()?;
    let client = Client::builder()
        .user_agent("property-scraping/0.1")
        .build()?;

    let mut all_listings = Vec::new();
    let base_url = Url::parse(&config.url)?;

    let first_page_html = client.get(config.url.clone()).send()?.error_for_status()?.text()?;
    let first_page = parse_next_data(&first_page_html)?;
    let first_page_props = first_page.props.page_props;

    let total_pages = first_page_props.pagination.total_pages;
    let total_count = first_page_props.pagination.total_count;
    let pages_to_scrape = if config.scrape_all {
        total_pages
    } else {
        config.max_pages.min(total_pages)
    };

    eprintln!("fetching page 1: {}", config.url);
    all_listings.extend(
        first_page_props
            .listings
            .into_iter()
            .map(|listing| to_scraped_listing(listing, &base_url)),
    );

    for page in 2..=pages_to_scrape {
        let page_url = page_url(&config.url, page)?;
        eprintln!("fetching page {page}: {page_url}");

        let html = client.get(page_url).send()?.error_for_status()?.text()?;
        let next_data = parse_next_data(&html)?;
        let page_props = next_data.props.page_props;

        all_listings.extend(
            page_props
                .listings
                .into_iter()
                .map(|listing| to_scraped_listing(listing, &base_url)),
        );
    }

    let result = ScrapeResult {
        source_url: config.url,
        pages_scraped: pages_to_scrape,
        total_pages,
        total_count,
        listings: all_listings,
    };

    println!("{}", serde_json::to_string_pretty(&result)?);
    Ok(())
}

struct Config {
    url: String,
    max_pages: u32,
    scrape_all: bool,
}

impl Config {
    fn from_args() -> Result<Self, Box<dyn Error>> {
        let mut url = DEFAULT_URL.to_string();
        let mut max_pages = 1u32;
        let mut scrape_all = false;

        let mut args = env::args().skip(1);
        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--url" => {
                    url = args.next().ok_or("--url requires a value")?;
                }
                "--max-pages" => {
                    let value = args.next().ok_or("--max-pages requires a value")?;
                    max_pages = value.parse()?;
                }
                "--all" => {
                    scrape_all = true;
                }
                "--help" | "-h" => {
                    print_help();
                    std::process::exit(0);
                }
                other => {
                    return Err(format!("unknown arg: {other}").into());
                }
            }
        }

        Ok(Self {
            url,
            max_pages,
            scrape_all,
        })
    }
}

fn print_help() {
    eprintln!(
        "Usage: property_scraping [--url URL] [--max-pages N] [--all]\n\n\
         Defaults to the PropertyHub Huai Khwang rental listing page."
    );
}

fn parse_next_data(html: &str) -> Result<NextData, Box<dyn Error>> {
    let json = extract_next_data(html).ok_or(ScrapeError::MissingNextData)?;
    Ok(serde_json::from_str(json)?)
}

fn extract_next_data(html: &str) -> Option<&str> {
    let marker = r#"id="__NEXT_DATA__""#;
    let start = html.find(marker)?;
    let after_open_tag = html[start..].find('>')? + start + 1;
    let end = html[after_open_tag..].find("</script>")? + after_open_tag;
    Some(html[after_open_tag..end].trim())
}

fn page_url(base_url: &str, page: u32) -> Result<String, Box<dyn Error>> {
    if page <= 1 {
        return Ok(base_url.to_string());
    }

    let mut url = Url::parse(base_url).map_err(|_| ScrapeError::UnexpectedUrl(base_url.to_string()))?;
    let path = url.path().trim_end_matches('/');
    url.set_path(&format!("{path}/{page}"));
    Ok(url.to_string())
}

fn to_scraped_listing(listing: Listing, base_url: &Url) -> ScrapedListing {
    let detail_url = base_url
        .join(&format!("/en/listings/{}", listing.slug))
        .map(|url| url.to_string())
        .unwrap_or_else(|_| listing.slug.clone());

    ScrapedListing {
        id: listing.id,
        title: listing.title,
        url: detail_url,
        project_name: listing.project.name,
        project_slug: listing.project.slug,
        project_address: listing.project.address,
        monthly_rent_thb: listing.price.for_rent.and_then(|rent| rent.monthly.price),
        bedrooms: listing.room_information.number_of_bed,
        bathrooms: listing.room_information.number_of_bath,
        room_area_m2: listing.room_information.room_area,
        floor: listing.room_information.on_floor,
        room_type: listing.room_information.room_type,
        updated_at: listing.updated_at,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_next_data_from_html() {
        let html = r#"
            <html>
              <body>
                <script id="__NEXT_DATA__" type="application/json">
                  {"props":{"pageProps":{"listings":[],"pagination":{"page":1,"perPage":60,"totalCount":1,"totalPages":1}}}}
                </script>
              </body>
            </html>
        "#;

        let next_data = parse_next_data(html).expect("next data");
        assert_eq!(next_data.props.page_props.pagination.total_pages, 1);
    }

    #[test]
    fn builds_page_urls() {
        assert_eq!(
            page_url(DEFAULT_URL, 1).unwrap(),
            DEFAULT_URL.to_string()
        );
        assert_eq!(
            page_url(DEFAULT_URL, 2).unwrap(),
            "https://propertyhub.in.th/en/condo-for-rent/mrt-huai-khwang/2"
        );
    }
}
