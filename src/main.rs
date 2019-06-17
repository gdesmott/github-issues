extern crate github_rs;
use github_rs::client::{Executor, Github};
use github_rs::{HeaderMap, StatusCode};

#[macro_use]
extern crate serde_derive;
extern crate serde_json;

extern crate url;
use url::Url;

extern crate structopt;
use structopt::StructOpt;

extern crate csv;

extern crate itertools;
use itertools::Itertools;

use std::cmp::Ordering;
use std::path::PathBuf;

#[derive(Debug, Serialize, Deserialize)]
struct PullRequest {
    url: String,
    html_url: String,
    diff_url: String,
    patch_url: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct Assignee {
    login: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct Milestone {
    title: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct Label {
    name: String,
}

#[derive(Debug, Serialize, Deserialize, PartialEq)]
enum IssueStateJson {
    #[serde(rename = "open")]
    Open,
    #[serde(rename = "closed")]
    Closed,
}

#[derive(Debug, Eq, PartialEq, Ord, PartialOrd)]
enum IssueState {
    Blocked,
    UnderReview,
    Open,
    Closed,
}

#[derive(Debug, Serialize, Deserialize)]
struct Issue {
    title: String,
    html_url: String,
    number: u32,
    repository_url: String,
    pull_request: Option<PullRequest>,
    assignee: Option<Assignee>,
    milestone: Option<Milestone>,
    labels: Option<Vec<Label>>,
    state: IssueStateJson,
    created_at: String,
    closed_at: Option<String>,
}

#[derive(Debug, Serialize)]
struct IssueCSV<'a> {
    component: String,
    id: String,
    title: &'a str,
    state: String,
    assignee: Option<&'a str>,
    milestone: Option<&'a str>,
    priority: Option<u32>,
    created_at: &'a str,
    closed_at: Option<&'a str>,
    url: &'a str,
}

fn strip_date(d: &str) -> &str {
    // Keep only 'yyyy-mm-dd'
    &d[..10]
}

impl Issue {
    fn is_pull_request(&self) -> bool {
        self.pull_request.is_some()
    }

    fn get_component(&self) -> String {
        let url = Url::parse(&self.repository_url).expect("Failed to parse repo URL");
        let path_segments = url
            .path_segments()
            .expect("Failed to extract path segments");

        path_segments
            .last()
            .expect("missing path segment")
            .to_string()
    }

    fn csv(&self) -> IssueCSV {
        IssueCSV {
            component: self.get_component(),
            id: format!("#{}", self.number),
            title: &self.title,
            state: self.get_state_str(),
            assignee: {
                match self.assignee {
                    Some(ref a) => Some(&a.login),
                    None => None,
                }
            },
            milestone: {
                match self.milestone {
                    Some(ref m) => Some(&m.title),
                    None => None,
                }
            },
            priority: self.get_priority(),
            created_at: self.get_created_at(),
            closed_at: self.get_closed_at(),
            url: &self.html_url,
        }
    }

    fn get_priority(&self) -> Option<u32> {
        if self.labels.is_none() {
            return None;
        }

        for label in self.labels.as_ref().unwrap() {
            match label.name.as_str() {
                "P0" => return Some(0),
                "P1" => return Some(1),
                "P2" => return Some(2),
                "P3" => return Some(3),
                "P4" => return Some(4),
                "P5" => return Some(5),
                _ => continue,
            }
        }
        None
    }

    fn get_state(&self) -> IssueState {
        if self.state == IssueStateJson::Closed {
            return IssueState::Closed;
        }

        if let Some(labels) = self.labels.as_ref() {
            if labels.iter().any(|l| l.name == "under review") {
                return IssueState::UnderReview;
            } else if labels.iter().any(|l| l.name == "blocked") {
                return IssueState::Blocked;
            }
        }

        IssueState::Open
    }

    fn get_state_str(&self) -> String {
        match self.get_state() {
            IssueState::Open => "open".to_string(),
            IssueState::Closed => "closed".to_string(),
            IssueState::Blocked => "blocked".to_string(),
            IssueState::UnderReview => "under review".to_string(),
        }
    }

    fn get_created_at(&self) -> &str {
        strip_date(&self.created_at)
    }

    fn get_closed_at(&self) -> Option<&str> {
        match self.closed_at {
            None => None,
            Some(ref d) => Some(strip_date(&d)),
        }
    }
}

type Issues = Vec<Issue>;

fn get_json(
    response: Result<(HeaderMap, StatusCode, Option<Issues>), github_rs::errors::Error>,
) -> Option<Issues> {
    match response {
        Ok((_headers, _status, json)) => json,
        Err(e) => {
            println!("{}", e);
            None
        }
    }
}

fn get_issues(client: &Github, owner: &str, repo_name: &str) -> Option<Issues> {
    let issues_endpoint = format!(
        "repos/{}/{}/issues?state=all&per_page=500",
        owner, repo_name
    );
    let response = client
        .get()
        .custom_endpoint(&issues_endpoint)
        .execute::<Issues>();
    get_json(response)
}

#[derive(StructOpt)]
#[structopt(
    name = "github-issues",
    about = "Aggregate issues from various github repositories"
)]
struct Opt {
    #[structopt(help = "github auth token")]
    token: String,
    #[structopt(help = "owner of github components")]
    owner: String,
    #[structopt(
        help = "output file",
        short = "o",
        long = "output",
        default_value = "issues.csv",
        parse(from_os_str)
    )]
    output: PathBuf,
    #[structopt(help = "github components to look for issues")]
    components: Vec<String>,
}

fn get_all_issues(client: &Github, owner: &str, components: &[String]) -> Vec<Issue> {
    let mut issues: Vec<Issue> = Vec::new();

    for component in components {
        issues.append(&mut get_issues(&client, owner, &component).expect("failed to get issues"));
    }

    // Filter out pull requests
    let issues = issues.into_iter().filter(|i| !i.is_pull_request());

    issues.sorted_by(|a, b| {
        let state_a = a.get_state();
        let state_b = b.get_state();

        // Put closed tasks last
        match (&state_a, &state_b) {
            (&IssueState::Closed, _) => return Ordering::Greater,
            (_, &IssueState::Closed) => return Ordering::Less,
            _ => {}
        };

        match (a.get_priority(), b.get_priority()) {
            (Some(_a), None) => return Ordering::Less,
            (None, Some(_b)) => return Ordering::Greater,
            (Some(pa), Some(pb)) => return pa.cmp(&pb),
            _ => {}
        };

        if state_a != state_b {
            return state_a.cmp(&state_b);
        }

        if state_a == IssueState::Closed {
            return b.get_closed_at().cmp(&a.get_closed_at());
        }

        let cmp = a.get_component().cmp(&b.get_component());
        if cmp == Ordering::Less || cmp == Ordering::Greater {
            return cmp;
        }

        let cmp = a.number.cmp(&b.number);
        if cmp == Ordering::Less || cmp == Ordering::Greater {
            return cmp;
        }

        Ordering::Equal
    })
}

fn generate_csv(issues: Vec<Issue>, output: &PathBuf) {
    let mut wtr = csv::Writer::from_path(&output).expect("Failed to create output file");

    for issue in issues.into_iter() {
        println!("{:?} {}", issue, issue.get_component());
        wtr.serialize(issue.csv()).expect("Failed to add record");
    }

    wtr.flush().expect("Failed to flush output");
}

fn main() {
    let opt = Opt::from_args();

    let client = Github::new(opt.token).unwrap();
    let issues = get_all_issues(&client, &opt.owner, &opt.components);

    generate_csv(issues, &opt.output);
}
