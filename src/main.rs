extern crate chrono;
extern crate config;
#[macro_use]
extern crate failure;
extern crate hubcaps;
extern crate serde;
#[macro_use]
extern crate serde_derive;
extern crate tokio_core;
extern crate unicorn_hat_hd;

use failure::Error;
use hubcaps::{Credentials, Github};
use hubcaps::issues;
use hubcaps::issues::IssueListOptionsBuilder;
use tokio_core::reactor::Core;
use unicorn_hat_hd::{UnicornHatHd, Rotate};

mod settings;
mod display;

use std::thread;
use std::time::Duration;

use display::{MetricType, RGB};

fn main() {
    let mut uhd = setup_unicorn_hat_hd();

    let settings = match settings::Settings::new() {
        Ok(s) => s,
        Err(e) => panic!("Could not read settings: {}", e),
    };

    loop {
        update_display(&settings, &mut uhd);
        thread::sleep(Duration::from_secs(600));
    }
}

fn update_display(settings: &settings::Settings, mut uhd: &mut UnicornHatHd) {
    let token = settings.github_token.clone();

    let mut core = Core::new().expect("reactor fail");
    let github = Github::new(
        concat!(env!("CARGO_PKG_NAME"), "/", env!("CARGO_PKG_VERSION")),
        Some(Credentials::Token(token)),
        &core.handle(),
    );

    let mut metrics = vec![];

    for repo in &settings.repositories {
        let hubcap_repo = github.repo(repo.user.clone(), repo.name.clone());
        let mut open_issues = 0;
        let mut closed_issues = 0;
        let mut merged_issues = 0;
        let mut assigned_open_issues = 0;

        // Gather information about the open issues.
        for issue in core.run(hubcap_repo.issues().list(&{
            let mut ilo = IssueListOptionsBuilder::new();
            ilo.state(issues::State::Open);
            if let Some(ref l) = repo.labels {
                ilo.labels(l.clone());
            };

            ilo.build()
        })).unwrap() {
            open_issues += 1;

            if let Some(_) = issue.assignee {
                if issue.closed_at == None {
                    assigned_open_issues += 1;
                }
            };
        }

        // Gather information about the closed issues that were updated more
        // recently than repo.since.
        for issue in core.run(hubcap_repo.issues().list(&{
            let mut ilo = IssueListOptionsBuilder::new();
            ilo.state(issues::State::Closed);
            if let Some(ref l) = repo.labels {
                ilo.labels(l.clone());
            };
            if let Some(ref s) = repo.since {
                ilo.since(s.clone());
            }

            ilo.build()
        })).unwrap() {
            closed_issues += 1;

            let pr = core.run(hubcap_repo.pulls().get(issue.number).get()).unwrap();
            if let Some(_) = pr.merged_at {
                merged_issues += 1;
            }
        }

        println!("Summary ({} issues):", open_issues + closed_issues);
        println!("\tOpen: {}", open_issues);
        println!("\tClosed: {}", closed_issues);
        println!("\tMerged: {}", merged_issues);
        println!("\tAssigned: {}", assigned_open_issues);

        metrics.push(MetricType::ColumnRatio {
            width: 1,
            values: vec![open_issues, closed_issues - merged_issues, merged_issues],
            colors: vec![RGB::new(0, 255, 0), RGB::new(0, 0, 255), RGB::new(191, 119, 246)]
        });

        metrics.push(MetricType::ColumnRatio {
            width: 1,
            values: vec![open_issues - assigned_open_issues, assigned_open_issues],
            colors: vec![RGB::new(12,255,12), RGB::new(2,171,46)]
        });
    }

    display_metrics(&mut uhd, metrics);
}

fn setup_unicorn_hat_hd() -> UnicornHatHd {
    let mut uhd = UnicornHatHd::default();
    uhd.set_rotation(Rotate::Rot180);

    uhd
}

fn fill_column(uhd: &mut UnicornHatHd, col: usize, colors: Vec<RGB>) -> Result<(), Error> {
    if colors.len() > 16 {
        return Err(format_err!("Number of values ({}) cannot exceed 16.", colors.len()));
    }

    for (i, &c) in colors.iter().enumerate() {
        uhd.set_pixel(col, 15 - i, c.r, c.g, c.b);
    }

    Ok(())
}

fn fill_column_ratio(mut uhd: &mut UnicornHatHd, col: usize, vals: Vec<u32>, colors: Vec<RGB>) -> Result<(), Error> {
    if vals.len() != colors.len() {
        return Err(format_err!("Number of values ({}) does not match number of colors ({}).", vals.len(), colors.len()));
    }

    let mut leds = vec![];
    let total: u32 = vals.iter().sum();
    for (i, &v) in vals.iter().enumerate() {
        let num_leds = if total > 16 {
            (f64::from(v) * 17f64 / (f64::from(total) + 1f64)) as u64
        } else {
            (f64::from(v) * 16f64 / f64::from(total)) as u64
        };
        for _ in 0..num_leds {
            leds.push(colors[i].clone());
        }
    }

    fill_column(&mut uhd, col, leds)
}

fn display_metrics(mut uhd: &mut UnicornHatHd, metrics: Vec<MetricType>) -> Result<(), Error> {
    let mut current_column = 0;
    for metric in metrics {
        match metric {
            MetricType::ColumnRatio { width: width @ _, values: values @ _, colors: colors @ _ } => {
                for i in 0..width {
                    fill_column_ratio(&mut uhd, current_column, values.clone(), colors.clone())?;
                    current_column += 1;
                }
            },
            MetricType::ColumnCount { width: _, value: _ } => unimplemented!(),
        }
    }
    uhd.display()?;
    Ok(())
}
