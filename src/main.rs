extern crate github_rs;
use github_rs::client::{Executor, Github};
use github_rs::{Headers, StatusCode};

#[macro_use]
extern crate serde_derive;
extern crate serde_json;

extern crate url;
use url::Url;

#[macro_use]
extern crate structopt;
use structopt::StructOpt;

extern crate csv;

extern crate itertools;
use itertools::Itertools;

use std::path::PathBuf;
use std::cmp::Ordering;

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
}

#[derive(Debug, Serialize)]
struct IssueCSV<'a> {
    component: String,
    id: String,
    title: &'a str,
    assignee: Option<&'a str>,
    milestone: Option<&'a str>,
    priority: Option<u32>,
    url: &'a str,
}

impl Issue {
    fn is_pull_request(&self) -> bool {
        self.pull_request.is_some()
    }

    fn get_component(&self) -> String {
        let url = Url::parse(&self.repository_url).expect("Failed to parse repo URL");
        let path_segments = url.path_segments()
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
}

type Issues = Vec<Issue>;

fn get_json(
    response: Result<(Headers, StatusCode, Option<Issues>), github_rs::errors::Error>,
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
    let response = client
        .get()
        .repos()
        .owner(owner)
        .repo(repo_name)
        .issues()
        .execute::<Issues>();
    get_json(response)
}

#[derive(StructOpt)]
#[structopt(name = "github-issues", about = "Aggregate issues from various github repositories")]
struct Opt {
    #[structopt(help = "github auth token")] token: String,
    #[structopt(help = "owner of github components")] owner: String,
    #[structopt(help = "output file", short = "o", long = "output", default_value = "issues.csv",
                parse(from_os_str))]
    output: PathBuf,
    #[structopt(help = "github components to look for issues")] components: Vec<String>,
}

fn main() {
    let opt = Opt::from_args();

    let client = Github::new(opt.token).unwrap();
    let mut wtr = csv::Writer::from_path(&opt.output).expect("Failed to create output file");

    for component in opt.components {
        let issues = get_issues(&client, &opt.owner, &component).expect("failed to get issues");
        // Filter out pull requests
        let issues = issues.into_iter().filter(|i| !i.is_pull_request());
        // Sort by priorities
        let issues = issues
            .sorted_by(|a, b| match (a.get_priority(), b.get_priority()) {
                (Some(_a), None) => Ordering::Less,
                (None, Some(_b)) => Ordering::Greater,
                (Some(pa), Some(pb)) => pa.cmp(&pb),
                _ => Ordering::Equal,
            })
            .into_iter();

        for issue in issues {
            println!("{:?} {}", issue, issue.get_component());
            wtr.serialize(issue.csv()).expect("Failed to add record");
        }
    }

    wtr.flush().expect("Failed to flush output");
}
