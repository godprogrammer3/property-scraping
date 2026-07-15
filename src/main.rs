use std::collections::HashMap;
use std::env;
use std::error::Error;
use std::io::{self, Write};
use std::time::Duration;

use reqwest::blocking::Client;
use reqwest::header::{
    HeaderMap, HeaderValue, ACCEPT, ACCEPT_ENCODING, ACCEPT_LANGUAGE, CACHE_CONTROL, CONTENT_TYPE,
    ORIGIN, PRAGMA, REFERER, USER_AGENT,
};
use serde::{Deserialize, Serialize};
use serde_json::json;

const GRAPHQL_URL: &str = "https://api.propertyhub.in.th/graphql";
const DEFAULT_LOCALE: &str = "TH";
const DEFAULT_PER_PAGE: u32 = 60;
const DEFAULT_ORDER: &str = "REFRESHED_AT";
const PERSISTED_QUERY_HASH: &str =
    "0583b83bf32bf57a9f58948244f716f1829de4f824e5fc7d75d550404dadaec8";
const PROJECT_URL_PREFIX: &str = "https://propertyhub.in.th/en/projects/";
const OSRM_BASE_URL_ENV: &str = "OSRM_BASE_URL";
const DEFAULT_OSRM_BASE_URL: &str = "https://router.project-osrm.org";

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
struct MrtStationsFile {
    stations: Vec<MrtStation>,
}

#[derive(Debug, Clone, Deserialize)]
struct MrtStation {
    #[serde(rename = "id")]
    _id: String,
    #[serde(rename = "poiId")]
    _poi_id: String,
    name: String,
    location: MrtStationLocation,
}

#[derive(Debug, Clone, Deserialize)]
struct MrtStationLocation {
    latitude: f64,
    longitude: f64,
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

#[derive(Debug, Clone, Default)]
struct ProjectDetails {
    property_type: Option<String>,
    developer: Option<String>,
    address: Option<String>,
    total_units: Option<u32>,
    ceiling_height_m: Option<String>,
    year_built: Option<u32>,
    common_fee_thb_per_sqm: Option<String>,
    number_of_buildings: Option<u32>,
    number_of_floors: Option<String>,
    structure_check: Option<String>,
    last_update: Option<String>,
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
    project_property_type: Option<String>,
    project_developer: Option<String>,
    project_total_units: Option<u32>,
    project_ceiling_height_m: Option<String>,
    project_year_built: Option<u32>,
    project_common_fee_thb_per_sqm: Option<String>,
    project_number_of_buildings: Option<u32>,
    project_number_of_floors: Option<String>,
    project_structure_check: Option<String>,
    project_last_update: Option<String>,
    project_rent_count: Option<u32>,
    project_sale_count: Option<u32>,
    near_mrt: Option<String>,
    distance_m: Option<f64>,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OutputFormat {
    Json,
    Csv,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NearMrtPreset {
    All,
    NearHuaikhwang,
    NearSuttisan,
    NearMrtRatchadaPisak,
    NearLadPhrao,
}

impl NearMrtPreset {
    fn parse(value: &str) -> Result<Self, DynError> {
        match value {
            "ALL" => Ok(Self::All),
            "NearHuaikhwang" => Ok(Self::NearHuaikhwang),
            "NearSuttisan" => Ok(Self::NearSuttisan),
            "NearMrtRatchadaPisak" => Ok(Self::NearMrtRatchadaPisak),
            "NearLadPhrao" => Ok(Self::NearLadPhrao),
            other => Err(format!("unknown near-mrt preset: {other}").into()),
        }
    }

    fn jobs(self) -> Vec<NearMrtPreset> {
        match self {
            Self::All => vec![
                Self::NearHuaikhwang,
                Self::NearSuttisan,
                Self::NearMrtRatchadaPisak,
                Self::NearLadPhrao,
            ],
            other => vec![other],
        }
    }

    fn zone_id(self) -> &'static str {
        match self {
            Self::NearHuaikhwang => "313",
            Self::NearSuttisan => "310",
            Self::NearMrtRatchadaPisak => "322",
            Self::NearLadPhrao => "309",
            Self::All => "ALL",
        }
    }

    fn max_price(self) -> u32 {
        match self {
            Self::NearHuaikhwang => 10_000,
            Self::NearSuttisan => 12_000,
            Self::NearMrtRatchadaPisak => 12_000,
            Self::NearLadPhrao => 12_000,
            Self::All => 12_000,
        }
    }

    fn room_type(self) -> Option<&'static str> {
        match self {
            Self::All => Some("ONE_BED_ROOM"),
            Self::NearHuaikhwang => Some("ONE_BED_ROOM"),
            Self::NearSuttisan => Some("ONE_BED_ROOM"),
            Self::NearMrtRatchadaPisak => Some("ONE_BED_ROOM"),
            Self::NearLadPhrao => Some("ONE_BED_ROOM"),
        }
    }
}

fn main() -> Result<(), DynError> {
    let config = Config::from_args()?;
    let client = Client::builder()
        .timeout(Duration::from_secs(60))
        .default_headers(default_headers(&config)?)
        .build()?;

    let mrt_stations = load_mrt_stations()?;
    let osrm_base_url =
        env::var(OSRM_BASE_URL_ENV).unwrap_or_else(|_| DEFAULT_OSRM_BASE_URL.to_string());

    let mut raw_listings = Vec::new();
    let mut total_pages = 0u32;
    let mut total_count = 0u32;
    let mut pages_scraped = 0u32;

    for preset in config.near_mrt_preset.jobs() {
        let job_config = config.for_preset(preset);
        let first_page = fetch_zone_page(&client, &job_config, 1)?;
        total_pages += first_page.pagination.total_pages;
        total_count += first_page.pagination.total_count;
        let pages_to_scrape = if job_config.scrape_all {
            first_page.pagination.total_pages
        } else {
            job_config.max_pages.min(first_page.pagination.total_pages)
        };
        pages_scraped += pages_to_scrape;

        raw_listings.extend(first_page.result);

        for page in 2..=pages_to_scrape {
            let response = fetch_zone_page(&client, &job_config, page)?;
            raw_listings.extend(response.result);
        }
    }

    let mut seen_ids = std::collections::HashSet::new();
    raw_listings.retain(|listing| seen_ids.insert(listing.id.clone()));

    let project_details = if config.include_project_details {
        load_project_details(&client, &raw_listings)?
    } else {
        HashMap::new()
    };

    let listings = raw_listings
        .into_iter()
        .map(|listing| {
            let project_slug = listing.project.slug.clone();
            let details = project_details.get(&project_slug);
            to_scraped_listing(listing, details, &mrt_stations, &client, &osrm_base_url)
        })
        .collect();

    let output = ScrapeResult {
        source: GRAPHQL_URL.to_string(),
        zone_id: config.zone_id.clone(),
        locale: config.locale,
        pages_scraped,
        total_pages,
        total_count,
        listings,
    };

    match config.output_format {
        OutputFormat::Json => {
            println!("{}", serde_json::to_string_pretty(&output)?);
        }
        OutputFormat::Csv => {
            write_csv(&output, &mut io::stdout())?;
        }
    }

    Ok(())
}

struct Config {
    near_mrt_preset: NearMrtPreset,
    zone_id: String,
    zone_id_override: Option<String>,
    locale: String,
    max_price: Option<u32>,
    max_price_override: Option<u32>,
    room_type: Option<String>,
    per_page: u32,
    order: String,
    max_pages: u32,
    scrape_all: bool,
    output_format: OutputFormat,
    include_project_details: bool,
}

impl Config {
    fn from_args() -> Result<Self, DynError> {
        let mut near_mrt_preset = NearMrtPreset::NearHuaikhwang;
        let mut zone_id = None::<String>;
        let mut zone_id_override = None::<String>;
        let mut locale = DEFAULT_LOCALE.to_string();
        let mut max_price = None::<u32>;
        let mut max_price_override = None::<u32>;
        let mut room_type = None::<String>;
        let mut per_page = DEFAULT_PER_PAGE;
        let mut order = DEFAULT_ORDER.to_string();
        let mut max_pages = 1u32;
        let mut scrape_all = false;
        let mut output_format = OutputFormat::Json;
        let mut include_project_details = true;
        let mut max_price_explicit = false;

        let mut args = env::args().skip(1);
        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--near-mrt" => {
                    near_mrt_preset = NearMrtPreset::parse(
                        &args.next().ok_or("--near-mrt requires a value")?,
                    )?
                }
                "--zone-id" => {
                    let value = args.next().ok_or("--zone-id requires a value")?;
                    zone_id = Some(value.clone());
                    zone_id_override = Some(value);
                }
                "--locale" => locale = args.next().ok_or("--locale requires a value")?,
                "--max-price" => {
                    let value = args.next().ok_or("--max-price requires a value")?;
                    let parsed = value.parse()?;
                    max_price = Some(parsed);
                    max_price_override = Some(parsed);
                    max_price_explicit = true;
                }
                "--no-max-price" => {
                    max_price = None;
                    max_price_override = None;
                    max_price_explicit = true;
                }
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
                "--format" => {
                    let value = args.next().ok_or("--format requires a value")?;
                    output_format = match value.as_str() {
                        "json" => OutputFormat::Json,
                        "csv" => OutputFormat::Csv,
                        other => return Err(format!("unknown format: {other}").into()),
                    };
                }
                "--csv" => output_format = OutputFormat::Csv,
                "--json" => output_format = OutputFormat::Json,
                "--no-project-details" => include_project_details = false,
                "--help" | "-h" => {
                    print_help();
                    std::process::exit(0);
                }
                other => return Err(format!("unknown arg: {other}").into()),
            }
        }

        let zone_id = zone_id.unwrap_or_else(|| near_mrt_preset.zone_id().to_string());
        let max_price = if max_price_explicit {
            max_price
        } else {
            Some(near_mrt_preset.max_price())
        };
        if room_type.is_none() {
            room_type = near_mrt_preset.room_type().map(ToString::to_string);
        }

        Ok(Self {
            near_mrt_preset,
            zone_id,
            zone_id_override,
            locale,
            max_price,
            max_price_override,
            room_type,
            per_page,
            order,
            max_pages,
            scrape_all,
            output_format,
            include_project_details,
        })
    }

    fn for_preset(&self, preset: NearMrtPreset) -> Self {
        let mut cloned = Self {
            near_mrt_preset: preset,
            zone_id: preset.zone_id().to_string(),
            zone_id_override: None,
            locale: self.locale.clone(),
            max_price: Some(preset.max_price()),
            max_price_override: None,
            room_type: preset.room_type().map(ToString::to_string),
            per_page: self.per_page,
            order: self.order.clone(),
            max_pages: self.max_pages,
            scrape_all: self.scrape_all,
            output_format: self.output_format,
            include_project_details: self.include_project_details,
        };

        if let Some(zone_id) = &self.zone_id_override {
            cloned.zone_id = zone_id.clone();
            cloned.zone_id_override = Some(zone_id.clone());
        }
        if let Some(max_price) = self.max_price_override {
            cloned.max_price = Some(max_price);
            cloned.max_price_override = Some(max_price);
        } else if self.max_price.is_none() {
            cloned.max_price = None;
        }
        if self.room_type.is_some() {
            cloned.room_type = self.room_type.clone();
        }

        cloned
    }
}

fn print_help() {
    eprintln!(
        "Usage: property_scraping [--near-mrt NearHuaikhwang|NearSuttisan|NearMrtRatchadaPisak|NearLadPhrao|ALL] [--zone-id ID] [--locale TH] [--max-price N|--no-max-price] [--per-page N] [--order ORDER] [--max-pages N] [--all] [--format json|csv] [--no-project-details]"
    );
}

fn default_headers(config: &Config) -> Result<HeaderMap, DynError> {
    let mut headers = HeaderMap::new();
    headers.insert(
        USER_AGENT,
        HeaderValue::from_static(
            "Mozilla/5.0 (X11; Ubuntu; Linux x86_64; rv:151.0) Gecko/20100101 Firefox/151.0",
        ),
    );
    headers.insert(ACCEPT, HeaderValue::from_static("*/*"));
    headers.insert(ACCEPT_LANGUAGE, HeaderValue::from_static("en-US,en;q=0.9"));
    headers.insert(
        ACCEPT_ENCODING,
        HeaderValue::from_static("gzip, deflate, br, zstd"),
    );
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
                "roomInformation": config
                    .room_type
                    .as_ref()
                    .map(|room_type| json!({ "roomType": room_type }))
                    .unwrap_or_else(|| json!({})),
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

fn load_project_details(
    client: &Client,
    listings: &[Listing],
) -> Result<HashMap<String, ProjectDetails>, DynError> {
    let mut details_by_slug = HashMap::new();
    let mut seen = std::collections::HashSet::new();

    for listing in listings {
        let slug = listing.project.slug.clone();
        if !seen.insert(slug.clone()) {
            continue;
        }

        let details = fetch_project_details(client, &slug)?;
        details_by_slug.insert(slug, details);
    }

    Ok(details_by_slug)
}

fn fetch_project_details(client: &Client, slug: &str) -> Result<ProjectDetails, DynError> {
    let url = format!("{PROJECT_URL_PREFIX}{slug}");
    let html = client.get(url).send()?.error_for_status()?.text()?;
    Ok(parse_project_details(&html))
}

fn parse_project_details(html: &str) -> ProjectDetails {
    let text = html_to_text(html);
    ProjectDetails {
        property_type: clean_text_value(extract_between(&text, "Property Type", "Developer")),
        developer: clean_text_value(extract_between(&text, "Developer", "Address")),
        address: clean_text_value(extract_between(&text, "Address", "Total Units")),
        total_units: extract_between(&text, "Total Units", "Ceiling Height (m)")
            .and_then(|value| parse_u32(clean_numeric(&value))),
        ceiling_height_m: clean_text_value(
            extract_between(&text, "Ceiling Height (m)", "Year Built"),
        ),
        year_built: extract_between(&text, "Year Built", "Common Fee")
            .and_then(|value| parse_u32(clean_numeric(&value))),
        common_fee_thb_per_sqm: clean_text_value(extract_between(
            &text,
            "Common Fee",
            "Number of Buildings",
        )),
        number_of_buildings: extract_between(&text, "Number of Buildings", "Number of Floors")
            .and_then(|value| parse_u32(clean_numeric(&value))),
        number_of_floors: clean_text_value(extract_between_any(
            &text,
            "Number of Floors",
            &["Structure Check", "This project is developed by", "Project Location"],
        )),
        structure_check: clean_text_value(extract_between_any(
            &text,
            "Structure Check",
            &["Last Update :", "This project is developed by", "Project Location"],
        )),
        last_update: clean_text_value(extract_between_any(
            &text,
            "Last Update :",
            &["View Evidence", "This project is developed by", "Project Location"],
        )),
    }
}

fn to_scraped_listing(
    listing: Listing,
    details: Option<&ProjectDetails>,
    mrt_stations: &[MrtStation],
    client: &Client,
    osrm_base_url: &str,
) -> ScrapedListing {
    let Project {
        _id: _,
        name,
        name_english,
        address,
        slug,
        listing_count_by_post_type,
    } = listing.project;

    let project_url = format!("{PROJECT_URL_PREFIX}{slug}");
    let detail_url = format!(
        "https://propertyhub.in.th/en/listings/{}---{}",
        listing.slug, listing.id
    );
    let project_counts = listing_count_by_post_type;
    let details = details.cloned().unwrap_or_default();
    let (near_mrt, distance_m) = match listing.location.as_ref() {
        Some(location) => nearest_mrt_and_distance(location, mrt_stations, client, osrm_base_url)
            .map(|value| (Some(value.station_name), Some(value.distance_m)))
            .unwrap_or((None, None)),
        None => (None, None),
    };

    ScrapedListing {
        id: listing.id,
        title: listing.title,
        url: detail_url,
        project_name: name,
        project_name_english: name_english,
        project_slug: slug,
        project_url,
        project_address: address.or(details.address.clone()),
        project_property_type: details.property_type,
        project_developer: details.developer,
        project_total_units: details.total_units,
        project_ceiling_height_m: details.ceiling_height_m,
        project_year_built: details.year_built,
        project_common_fee_thb_per_sqm: details.common_fee_thb_per_sqm,
        project_number_of_buildings: details.number_of_buildings,
        project_number_of_floors: details.number_of_floors,
        project_structure_check: details.structure_check,
        project_last_update: details.last_update,
        project_rent_count: project_counts
            .as_ref()
            .and_then(|counts| counts.for_rent.as_ref())
            .and_then(|count| count.listing_count),
        project_sale_count: project_counts
            .as_ref()
            .and_then(|counts| counts.for_sale.as_ref())
            .and_then(|count| count.listing_count),
        near_mrt,
        distance_m,
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

fn write_csv(output: &ScrapeResult, writer: &mut impl Write) -> Result<(), DynError> {
    let headers = [
        "Id",
        "Name",
        "Link",
        "Built",
        "Rent Price",
        "Near MRT",
        "Distance(m)",
        "Area(m*m)",
        "Fl",
        "Total Fl",
        "Direction",
        "Parking",
        "Note",
    ];

    writeln!(writer, "{}", headers.join(","))?;

    for listing in &output.listings {
        let row = [
            csv_cell(Some(&listing.id)),
            csv_cell(Some(&listing.project_name)),
            csv_cell(Some(&listing.url)),
            csv_cell_string(listing.project_year_built.map(|v| v.to_string())),
            csv_cell_string(listing.monthly_rent_thb.map(format_baht)),
            csv_cell(listing.near_mrt.as_deref()),
            csv_cell_string(listing.distance_m.map(format_distance)),
            csv_cell_string(listing.room_area_m2.map(trim_float)),
            csv_cell(listing.floor.as_deref()),
            csv_cell(listing.project_number_of_floors.as_deref()),
            csv_cell_empty(),
            csv_cell_empty(),
            csv_cell(listing.room_type.as_deref()),
        ];

        writeln!(writer, "{}", row.join(","))?;
    }

    Ok(())
}

fn csv_cell(value: Option<&str>) -> String {
    match value {
        None => String::new(),
        Some(value) => {
            let needs_quotes = value.contains(',') || value.contains('"') || value.contains('\n');
            let escaped = value.replace('"', "\"\"");
            if needs_quotes {
                format!("\"{escaped}\"")
            } else {
                escaped
            }
        }
    }
}

fn csv_cell_string(value: Option<String>) -> String {
    match value {
        Some(value) => csv_cell(Some(&value)),
        None => String::new(),
    }
}

fn csv_cell_empty() -> String {
    String::new()
}

fn format_baht(value: f64) -> String {
    let rounded = format!("{value:.2}");
    let mut parts = rounded.split('.');
    let int_part = parts.next().unwrap_or_default();
    let frac_part = parts.next().unwrap_or("00");
    let mut grouped = String::new();

    for (idx, ch) in int_part.chars().rev().enumerate() {
        if idx != 0 && idx % 3 == 0 {
            grouped.push(',');
        }
        grouped.push(ch);
    }

    let int_grouped: String = grouped.chars().rev().collect();
    format!("฿{int_grouped}.{frac_part}")
}

fn trim_float(value: f64) -> String {
    let text = format!("{value}");
    text.strip_suffix(".0").unwrap_or(&text).to_string()
}

fn format_distance(value: f64) -> String {
    format!("{value:.2}")
}

#[derive(Debug)]
struct MrtRouteResult {
    station_name: String,
    distance_m: f64,
}

fn load_mrt_stations() -> Result<Vec<MrtStation>, DynError> {
    let data: MrtStationsFile = serde_json::from_str(include_str!("mrt_stations.json"))?;
    Ok(data.stations)
}

fn nearest_mrt_and_distance(
    room: &Location,
    stations: &[MrtStation],
    client: &Client,
    osrm_base_url: &str,
) -> Result<MrtRouteResult, DynError> {
    let nearest = stations
        .iter()
        .min_by(|left, right| {
            let left_distance = haversine_distance_m(
                room.lat,
                room.lng,
                left.location.latitude,
                left.location.longitude,
            );
            let right_distance = haversine_distance_m(
                room.lat,
                room.lng,
                right.location.latitude,
                right.location.longitude,
            );
            left_distance
                .partial_cmp(&right_distance)
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .ok_or_else(|| std::io::Error::other("no MRT stations available"))?;

    let distance_m = osrm_walk_distance_m(
        client,
        osrm_base_url,
        room.lat,
        room.lng,
        nearest.location.latitude,
        nearest.location.longitude,
    )
    .unwrap_or_else(|_| {
        haversine_distance_m(
            room.lat,
            room.lng,
            nearest.location.latitude,
            nearest.location.longitude,
        )
    });

    Ok(MrtRouteResult {
        station_name: mrt_display_name(&nearest.name),
        distance_m,
    })
}

fn mrt_display_name(name: &str) -> String {
    name.split_once(' ')
        .map(|(_, rest)| rest.to_string())
        .unwrap_or_else(|| name.to_string())
}

fn osrm_walk_distance_m(
    client: &Client,
    base_url: &str,
    from_lat: f64,
    from_lng: f64,
    to_lat: f64,
    to_lng: f64,
) -> Result<f64, DynError> {
    #[derive(Debug, Deserialize)]
    struct OsrmResponse {
        routes: Vec<OsrmRoute>,
    }

    #[derive(Debug, Deserialize)]
    struct OsrmRoute {
        distance: f64,
        #[allow(dead_code)]
        duration: f64,
    }

    let url = format!(
        "{}/route/v1/foot/{},{};{},{}?overview=false&steps=false",
        base_url, from_lng, from_lat, to_lng, to_lat
    );

    let response = client.get(&url).send()?.error_for_status()?;
    let parsed = response.json::<OsrmResponse>()?;
    let route = parsed
        .routes
        .first()
        .ok_or_else(|| std::io::Error::other("OSRM returned no routes"))?;
    Ok(route.distance)
}

fn haversine_distance_m(from_lat: f64, from_lng: f64, to_lat: f64, to_lng: f64) -> f64 {
    let earth_radius_m = 6_371_000.0;
    let d_lat = (to_lat - from_lat).to_radians();
    let d_lng = (to_lng - from_lng).to_radians();
    let lat1 = from_lat.to_radians();
    let lat2 = to_lat.to_radians();

    let a = (d_lat / 2.0).sin().powi(2)
        + lat1.cos() * lat2.cos() * (d_lng / 2.0).sin().powi(2);
    let c = 2.0 * a.sqrt().atan2((1.0 - a).sqrt());
    earth_radius_m * c
}

fn html_to_text(html: &str) -> String {
    let mut out = String::with_capacity(html.len());
    let bytes = html.as_bytes();
    let mut i = 0usize;
    let mut in_tag = false;
    let mut skip_until: Option<&'static [u8]> = None;

    while i < bytes.len() {
        if let Some(needle) = skip_until {
            if bytes[i..].starts_with(needle) {
                i += needle.len();
                skip_until = None;
                continue;
            }
            i += 1;
            continue;
        }

        if !in_tag && bytes[i] == b'<' {
            let lower = bytes[i..]
                .iter()
                .take(16)
                .map(|b| b.to_ascii_lowercase())
                .collect::<Vec<_>>();
            if lower.starts_with(b"<script") {
                skip_until = Some(b"</script>");
                i += 1;
                continue;
            }
            if lower.starts_with(b"<style") {
                skip_until = Some(b"</style>");
                i += 1;
                continue;
            }
            in_tag = true;
            i += 1;
            continue;
        }

        if in_tag {
            if bytes[i] == b'>' {
                in_tag = false;
            }
            i += 1;
            continue;
        }

        if bytes[i] == b'&' {
            if bytes[i..].starts_with(b"&amp;") {
                out.push('&');
                i += 5;
                continue;
            }
            if bytes[i..].starts_with(b"&nbsp;") {
                out.push(' ');
                i += 6;
                continue;
            }
            if bytes[i..].starts_with(b"&#x27;") {
                out.push('\'');
                i += 6;
                continue;
            }
            if bytes[i..].starts_with(b"&quot;") {
                out.push('"');
                i += 6;
                continue;
            }
        }

        out.push(bytes[i] as char);
        i += 1;
    }

    collapse_whitespace(&out)
}

fn collapse_whitespace(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut last_space = false;

    for ch in text.chars() {
        if ch.is_whitespace() {
            if !last_space {
                out.push(' ');
                last_space = true;
            }
        } else {
            out.push(ch);
            last_space = false;
        }
    }

    out.trim().to_string()
}

fn extract_between(text: &str, start: &str, end: &str) -> Option<String> {
    let start_idx = text.find(start)? + start.len();
    let rest = &text[start_idx..];
    let end_idx = rest.find(end)?;
    Some(rest[..end_idx].trim().to_string())
}

fn extract_between_any(text: &str, start: &str, ends: &[&str]) -> Option<String> {
    let start_idx = text.find(start)? + start.len();
    let rest = &text[start_idx..];
    let mut best_end = None;

    for end in ends {
        if let Some(end_idx) = rest.find(end) {
            best_end = match best_end {
                Some(current) if current <= end_idx => Some(current),
                _ => Some(end_idx),
            };
        }
    }

    best_end.map(|end_idx| rest[..end_idx].trim().to_string())
}

fn clean_text_value(value: Option<String>) -> Option<String> {
    let value = value?.trim().to_string();
    if value.is_empty() || value == "-" || value.starts_with("- ") {
        None
    } else {
        Some(value)
    }
}

fn clean_numeric(value: &str) -> String {
    value
        .chars()
        .filter(|ch| ch.is_ascii_digit())
        .collect::<String>()
}

fn parse_u32(value: String) -> Option<u32> {
    if value.is_empty() {
        None
    } else {
        value.parse().ok()
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

        let listing = to_scraped_listing(
            zone.result.into_iter().next().unwrap(),
            None,
            &[],
            &Client::new(),
            DEFAULT_OSRM_BASE_URL,
        );
        assert_eq!(listing.project_rent_count, Some(264));
        assert_eq!(listing.project_sale_count, Some(142));
        assert_eq!(listing.monthly_rent_thb, Some(10000.0));
        assert_eq!(listing.floor.as_deref(), Some("22"));
    }

    #[test]
    fn parses_project_page_details() {
        let html = r#"
        <html><body>
        Project Details Project Name XT HUAIKHWANG Other Names XT ห้วยขวาง XT HUAIKWANG เอ็กซ์ที ห้วยขวาง Other Names Property Type Condominium Developer SANSIRI Address Huai Khwang Bangkok Total Units 1404 Ceiling Height (m) - meter Year Built 2021 Common Fee - THB/sqm Number of Buildings 2 Number of Floors 14,43 Structure Check Safe Last Update : 10/4/2568 View Evidence Facility Area
        </body></html>
        "#;

        let parsed = parse_project_details(html);
        assert_eq!(parsed.property_type.as_deref(), Some("Condominium"));
        assert_eq!(parsed.developer.as_deref(), Some("SANSIRI"));
        assert_eq!(parsed.address.as_deref(), Some("Huai Khwang Bangkok"));
        assert_eq!(parsed.total_units, Some(1404));
        assert_eq!(parsed.year_built, Some(2021));
        assert_eq!(parsed.number_of_buildings, Some(2));
        assert_eq!(parsed.number_of_floors.as_deref(), Some("14,43"));
        assert_eq!(parsed.structure_check.as_deref(), Some("Safe"));
        assert_eq!(parsed.last_update.as_deref(), Some("10/4/2568"));
    }
}
