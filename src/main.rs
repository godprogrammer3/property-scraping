use std::env;
use std::collections::{HashMap, HashSet};
use std::error::Error;
use std::fmt;
use std::sync::Arc;
use std::time::Duration;

use reqwest::blocking::Client;
use rayon::prelude::*;
use regex::Regex;
use serde::{Deserialize, Serialize};
use url::Url;

const DEFAULT_URL: &str = "https://propertyhub.in.th/en/condo-for-rent/mrt-huai-khwang";
type DynError = Box<dyn Error + Send + Sync>;

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
    listings: ListingPage,
}

#[derive(Debug, Deserialize)]
struct ListingPage {
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
struct DetailNextData {
    props: DetailProps,
}

#[derive(Debug, Deserialize)]
struct DetailProps {
    #[serde(rename = "pageProps")]
    page_props: DetailPageProps,
}

#[derive(Debug, Deserialize)]
struct DetailPageProps {
    listing: DetailListing,
}

#[derive(Debug, Deserialize)]
struct DetailListing {
    #[serde(rename = "id")]
    _id: serde_json::Value,
    title: String,
    #[serde(rename = "slug")]
    _slug: String,
    #[serde(default)]
    detail: Option<String>,
    #[serde(default, rename = "totalView")]
    total_view: Option<u64>,
    project: Project,
    price: Price,
    #[serde(rename = "roomInformation")]
    room_information: RoomInformation,
    #[serde(default)]
    images: Vec<serde_json::Value>,
    #[serde(default)]
    address: Option<String>,
    #[serde(default)]
    location: Option<serde_json::Value>,
    #[serde(default, rename = "updatedAt")]
    updated_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Default)]
struct ProjectPageData {
    project_url: String,
    rent_count: Option<u32>,
    sale_count: Option<u32>,
    developer: Option<String>,
    property_type: Option<String>,
    address: Option<String>,
    total_units: Option<u32>,
    year_built: Option<u32>,
    number_of_buildings: Option<u32>,
    number_of_floors: Option<String>,
    structure_check: Option<String>,
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
    detail_url: String,
    project_name: String,
    project_slug: String,
    project_address: Option<String>,
    project_url: String,
    project_rent_count: Option<u32>,
    project_sale_count: Option<u32>,
    project_developer: Option<String>,
    project_property_type: Option<String>,
    project_total_units: Option<u32>,
    project_year_built: Option<u32>,
    project_number_of_buildings: Option<u32>,
    project_number_of_floors: Option<String>,
    project_structure_check: Option<String>,
    monthly_rent_thb: Option<f64>,
    bedrooms: Option<u32>,
    bathrooms: Option<u32>,
    room_area_m2: Option<f64>,
    floor: Option<String>,
    room_type: Option<String>,
    updated_at: Option<String>,
    total_view: Option<u64>,
    description: Option<String>,
    image_count: usize,
    address: Option<String>,
    location: Option<serde_json::Value>,
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

fn main() -> Result<(), DynError> {
    let config = Config::from_args()?;
    let client = Client::builder()
        .user_agent("property-scraping/0.1")
        .timeout(Duration::from_secs(60))
        .build()?;
    let pool = rayon::ThreadPoolBuilder::new().num_threads(4).build()?;

    let mut all_listings = Vec::new();
    let base_url = Url::parse(&config.url)?;

    let first_page_html = client.get(config.url.clone()).send()?.error_for_status()?.text()?;
    let first_page = parse_next_data(&first_page_html)?;
    let first_page_props = first_page.props.page_props;
    let first_page_listings = first_page_props.listings;

    let total_pages = first_page_listings.pagination.total_pages;
    let total_count = first_page_listings.pagination.total_count;
    let pages_to_scrape = if config.scrape_all {
        total_pages
    } else {
        config.max_pages.min(total_pages)
    };

    eprintln!("fetching page 1: {}", config.url);
    all_listings.extend(pool.install(|| {
        scrape_listing_page(&client, first_page_listings.listings, &base_url)
    })?);

    for page in 2..=pages_to_scrape {
        let page_url = page_url(&config.url, page)?;
        eprintln!("fetching page {page}: {page_url}");

        let html = client.get(page_url).send()?.error_for_status()?.text()?;
        let next_data = parse_next_data(&html)?;
        let page_props = next_data.props.page_props.listings;

        all_listings.extend(pool.install(|| {
            scrape_listing_page(&client, page_props.listings, &base_url)
        })?);
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
    fn from_args() -> Result<Self, DynError> {
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

fn parse_next_data(html: &str) -> Result<NextData, DynError> {
    let json = extract_next_data(html).ok_or(ScrapeError::MissingNextData)?;
    Ok(serde_json::from_str(json)?)
}

fn parse_detail_next_data(html: &str) -> Result<DetailNextData, DynError> {
    let json = extract_next_data(html).ok_or(ScrapeError::MissingNextData)?;
    Ok(serde_json::from_str(json)?)
}

fn parse_project_page(html: &str, project_url: &str) -> Result<ProjectPageData, DynError> {
    let project_section = extract_project_details_section(html).unwrap_or(html);

    Ok(ProjectPageData {
        project_url: project_url.to_string(),
        rent_count: capture_count(html, r#"(?i)For rent\s*\(?(?P<value>[0-9,]+)\)?"#),
        sale_count: capture_count(html, r#"(?i)For sale\s*\(?(?P<value>[0-9,]+)\)?"#),
        developer: capture_label_text(project_section, "Developer"),
        property_type: capture_label_text(project_section, "Property Type"),
        address: capture_label_text(project_section, "Address"),
        total_units: capture_label_u32(project_section, "Total Units"),
        year_built: capture_label_u32(project_section, "Year Built"),
        number_of_buildings: capture_label_u32(project_section, "Number of Buildings"),
        number_of_floors: capture_label_text(project_section, "Number of Floors"),
        structure_check: capture_label_text(project_section, "Structure Check"),
    })
}

fn extract_project_details_section(html: &str) -> Option<&str> {
    let start_marker = "<h2 class=\"text-font-color capsize leading-tight\">Project Details</h2>";
    let end_marker = "### This project is developed by";
    let start = html.find(start_marker)?;
    let end = html[start..].find(end_marker)? + start;
    Some(&html[start..end])
}

fn capture_count(html: &str, pattern: &str) -> Option<u32> {
    let regex = Regex::new(pattern).ok()?;
    let captures = regex.captures(html)?;
    let value = captures.name("value")?.as_str().replace(',', "");
    value.parse().ok()
}

fn capture_label_text(html: &str, label: &str) -> Option<String> {
    let pattern = format!(
        r#"(?s){}.*?<span[^>]*capsize">(?P<value>[^<]+)"#,
        regex::escape(label)
    );
    let regex = Regex::new(&pattern).ok()?;
    Some(regex.captures(html)?.name("value")?.as_str().trim().to_string())
}

fn capture_label_u32(html: &str, label: &str) -> Option<u32> {
    capture_label_text(html, label)?
        .replace(',', "")
        .parse()
        .ok()
}

fn extract_next_data(html: &str) -> Option<&str> {
    let marker = r#"id="__NEXT_DATA__""#;
    let start = html.find(marker)?;
    let after_open_tag = html[start..].find('>')? + start + 1;
    let end = html[after_open_tag..].find("</script>")? + after_open_tag;
    Some(html[after_open_tag..end].trim())
}

fn page_url(base_url: &str, page: u32) -> Result<String, DynError> {
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
        .join(&format!("/en/listings/{}---{}", listing.slug, listing.id))
        .map(|url| url.to_string())
        .unwrap_or_else(|_| listing.slug.clone());

    ScrapedListing {
        id: listing.id,
        title: listing.title,
        url: detail_url.clone(),
        detail_url,
        project_name: listing.project.name,
        project_slug: listing.project.slug,
        project_address: listing.project.address,
        project_url: String::new(),
        project_rent_count: None,
        project_sale_count: None,
        project_developer: None,
        project_property_type: None,
        project_total_units: None,
        project_year_built: None,
        project_number_of_buildings: None,
        project_number_of_floors: None,
        project_structure_check: None,
        monthly_rent_thb: listing.price.for_rent.and_then(|rent| rent.monthly.price),
        bedrooms: listing.room_information.number_of_bed,
        bathrooms: listing.room_information.number_of_bath,
        room_area_m2: listing.room_information.room_area,
        floor: listing.room_information.on_floor,
        room_type: listing.room_information.room_type,
        updated_at: listing.updated_at,
        total_view: None,
        description: None,
        image_count: 0,
        address: None,
        location: None,
    }
}

fn scrape_listing_page(
    client: &Client,
    listings: Vec<Listing>,
    base_url: &Url,
) -> Result<Vec<ScrapedListing>, DynError> {
    let mut project_slugs = HashSet::new();
    for listing in &listings {
        project_slugs.insert(listing.project.slug.clone());
    }

    let mut project_cache = HashMap::new();
    for project_slug in project_slugs {
        let project_url = base_url
            .join(&format!("/en/projects/{project_slug}"))
            .map(|url| url.to_string())?;
        let html = client.get(&project_url).send()?.error_for_status()?.text()?;
        let project_data = parse_project_page(&html, &project_url)?;
        project_cache.insert(project_slug, project_data);
    }

    let project_cache = Arc::new(project_cache);

    listings
        .into_par_iter()
        .map(|listing| {
            let summary = to_scraped_listing(listing, base_url);
            enrich_with_detail(client, summary, &project_cache)
        })
        .collect::<Result<Vec<_>, _>>()
}

fn enrich_with_detail(
    client: &Client,
    listing: ScrapedListing,
    project_cache: &Arc<HashMap<String, ProjectPageData>>,
) -> Result<ScrapedListing, DynError> {
    let html = client
        .get(&listing.detail_url)
        .send()?
        .error_for_status()?
        .text()?;
    let next_data = parse_detail_next_data(&html)?;
    let detail = next_data.props.page_props.listing;
    let project_data = project_cache
        .get(&detail.project.slug)
        .cloned()
        .unwrap_or_default();

    Ok(ScrapedListing {
        id: listing.id,
        title: detail.title,
        url: listing.url,
        detail_url: listing.detail_url,
        project_name: detail.project.name,
        project_slug: detail.project.slug,
        project_address: detail.project.address,
        project_url: project_data.project_url,
        project_rent_count: project_data.rent_count,
        project_sale_count: project_data.sale_count,
        project_developer: project_data.developer,
        project_property_type: project_data.property_type,
        project_total_units: project_data.total_units,
        project_year_built: project_data.year_built,
        project_number_of_buildings: project_data.number_of_buildings,
        project_number_of_floors: project_data.number_of_floors,
        project_structure_check: project_data.structure_check,
        monthly_rent_thb: detail.price.for_rent.and_then(|rent| rent.monthly.price),
        bedrooms: detail.room_information.number_of_bed,
        bathrooms: detail.room_information.number_of_bath,
        room_area_m2: detail.room_information.room_area,
        floor: detail.room_information.on_floor,
        room_type: detail.room_information.room_type,
        updated_at: detail.updated_at,
        total_view: detail.total_view,
        description: detail.detail,
        image_count: detail.images.len(),
        address: detail.address,
        location: detail.location,
    })
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
                  {"props":{"pageProps":{"listings":{"listings":[],"pagination":{"page":1,"perPage":60,"totalCount":1,"totalPages":1}}}}}
                </script>
              </body>
            </html>
        "#;

        let next_data = parse_next_data(html).expect("next data");
        assert_eq!(
            next_data.props.page_props.listings.pagination.total_pages,
            1
        );
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

    #[test]
    fn parses_project_page_data() {
        let html = r#"
            <html>
              <body>
                <div>For rent (1,045) listings, For sale (141) listings</div>
                <h2 class="text-font-color capsize leading-tight">Project Details</h2>
                <div class="rounded-lg overflow-hidden border border-line">
                  <div class="flex"><div><label>Project Name</label></div><div><span class="text-md text-font-color font-medium capsize">XT HUAIKHWANG</span></div></div>
                  <div class="flex"><div><label>Property Type</label></div><div><span class="text-md text-font-color font-medium capsize">Condominium</span></div></div>
                  <div class="flex"><div><label>Developer</label></div><div><a><span class="text-md font-medium capsize">SANSIRI</span></a></div></div>
                  <div class="flex"><div><label>Address</label></div><div><span class="text-md text-font-color font-medium capsize">Huai Khwang Bangkok</span></div></div>
                  <div class="flex"><div><label>Total Units</label></div><div><span class="text-md text-font-color font-medium capsize">1404</span></div></div>
                  <div class="flex"><div><label>Year Built</label></div><div><span class="text-md text-font-color font-medium capsize">2021</span></div></div>
                  <div class="flex"><div><label>Number of Buildings</label></div><div><span class="text-md text-font-color font-medium capsize">2</span></div></div>
                  <div class="flex"><div><label>Number of Floors</label></div><div><span class="text-md text-font-color font-medium capsize">14,43</span></div></div>
                  <div class="flex"><div><label>Structure Check</label></div><div><span class="text-md text-font-color font-medium capsize">Safe</span></div></div>
                </div>
                ### This project is developed by
              </body>
            </html>
        "#;

        let parsed = parse_project_page(html, "https://propertyhub.in.th/en/projects/xt-huaikhwang")
            .expect("project page");
        assert_eq!(parsed.rent_count, Some(1045));
        assert_eq!(parsed.sale_count, Some(141));
        assert_eq!(parsed.developer.as_deref(), Some("SANSIRI"));
        assert_eq!(parsed.total_units, Some(1404));
        assert_eq!(parsed.year_built, Some(2021));
    }
}
