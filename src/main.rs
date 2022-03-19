
use std::env::{self};
use std::str::FromStr;
use std::io::{prelude::*, BufReader, Write};
use std::process::Command;
use urlencoding::encode;
use clap::{Parser, Subcommand};
use hyper::{Client, Uri, Request, Method, Body};
use hyper_tls::HttpsConnector;
use serde::{Serialize, Deserialize};
extern crate scopeguard;

#[derive(Parser)]
#[clap(name = "rusgit")]
#[clap(about = "Pull Github Code From Gitee", version="0.9.0", long_about = None)]
struct Cli {
    #[clap(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Clones repos
    #[clap(arg_required_else_help = true)]
    Clone {
        /// The remote to clone
        remote: String,
    },
    /// Submodule Init, Update
    Submodule {
        #[clap(subcommand)]
        action: SubmoduleCmds,
    },
}

#[derive(Debug, Subcommand)]
enum SubmoduleCmds {
    /// submodule init
    Init {
    },
    /// submodule update
    Update {
    },
}

#[tokio::main]
async  fn main() {
    let args = Cli::parse();
    let res;
    match &args.command {
        Commands::Clone { remote } => {
            res = clone(remote).await;
        },
        Commands::Submodule { action} => {
            res = submodule(action).await
        }
    }
 
    match res {
        Ok(_) => (),
        Err(err) => print!("{}", err.msg)
    }

}

fn get_gitee_token() -> Result<String, GetGiteeError> {
    let result = env::var("GITEE_SESSION");
    let token_key="gitee-session-n=".to_string();
    match result {
        Ok(val) => return Ok(token_key + &val),
        Err(_) => return Err(GetGiteeError{code : ErrorCode::InvalidToken, msg: "cant find GITEE_SESSION in env".to_string()}),
    }
}

async fn clone(remote: &str) -> Result<(), GetGiteeError> {
    let gitee_repo_url = get_url_from_gitee(remote).await?;
    print!("Found mirror repo: {}\n", &gitee_repo_url);
    let git_status = Command::new("git").arg("clone").arg(gitee_repo_url).status();
    if git_status.is_err() {
        print!("run git error: {}", git_status.err().unwrap());
    }
    Ok(())
}

async fn submodule(cmd: &SubmoduleCmds)  -> Result<(), GetGiteeError> {
    match cmd {
        SubmoduleCmds::Init {} => {
            submodule_init().await?;
        },
        SubmoduleCmds::Update {} => {
            return submodule_update();
        }
    }
    Ok(())
}

async fn get_submodule_line(l: String) -> Result<String, GetGiteeError> {
    let start_pos = l.find("url = https");
    if start_pos == None {
        return Ok(l);
    }

    let start = l[0..start_pos.unwrap()+6].to_string();

    let url = &l[start_pos.unwrap()+6..];
    let new_url = get_url_from_gitee(url).await?;
    println!("{} ==> {}", url, new_url);
    Ok(start + &new_url)
}

async fn submodule_init() -> Result<(), GetGiteeError> {

    let gitmodule = ".gitmodules";
    let gitmodule_bak = ".gitmodules.bak";
    let gitmodule_tmp = ".gitmodules.tmp";
    let mut new_line = "\n";
    if cfg!(windows) {
        new_line = "\r\n";
    }


    if std::fs::metadata(gitmodule_bak).is_err() {
        let _ = std::fs::copy(gitmodule, gitmodule_bak);
    }

    let _guard = scopeguard::guard((), |_| {
        let _ = std::fs::remove_file(gitmodule_tmp);
    });

    let file = std::fs::File::open(gitmodule);
    if file.is_ok() {
        let output = std::fs::File::create(gitmodule_tmp);
        let mut output = match output {
            Ok(f) => f,
            Err(err) => return Err(GetGiteeError{code: ErrorCode::WriteSubModuleError, msg: std::format!("Write .Submodule.tmp : {}", err)}),
        };
        let reader = BufReader::new(file.unwrap());

        for line in reader.lines() {
            let l = line.expect("read gitmodules");
            let l = get_submodule_line(l).await?;
            let l = l + &new_line;
            output.write(l.as_bytes()).expect("write gitmodules.tmp");
        }
    }
    
    std::fs::rename(gitmodule_tmp, gitmodule).expect("move gitmodule.tmp to gitmodule");

    let git_status = Command::new("git").arg("submodule").arg("init").status();
    if git_status.is_err() {
        print!("run git error: {}", git_status.err().unwrap());
    }
    Ok(())
}

fn submodule_update() -> Result<(), GetGiteeError> {
    let git_status = Command::new("git").arg("submodule").arg("update").status();
    if git_status.is_err() {
        print!("run git error: {}", git_status.err().unwrap());
    }
    Ok(())
}


#[derive(Debug)]
struct GetGiteeError {
    code: ErrorCode,
    msg: String
}

#[derive(Debug)]
enum ErrorCode {
    InvalidLogin,
    RequestError,
    AccessGiteeUnknowError,
    InvalidToken,
    CanNotFoundRepo,
    ParseResponseError,
    WriteSubModuleError,
}

#[derive(Serialize, Deserialize, Debug)]
struct DupResponse {
    is_duplicate: bool,
    #[serde(default)]
    message: String
}

async fn get_url_from_gitee(remote: &str) -> Result<String, GetGiteeError> {
    let remote_uri = remote.parse::<Uri>();
    let _ = match remote_uri {
        Ok(uri) => uri,
        Err(_) => panic!("Invalid Github Remote: {}", remote),
    };

    let dup_api_url = String::from("https://gitee.com/projects/check_project_duplicate?import_url="); 
    let dup_api_url = dup_api_url + &encode(remote).into_owned();
    let uri = Uri::from_str(&dup_api_url).unwrap();

    let https = HttpsConnector::new();
    let client = Client::builder().build::<_, hyper::Body>(https);

    let gitee_token = get_gitee_token()?;

    let req = Request::builder()
                            .method(Method::GET).uri(uri)
                            .header("UserAgent", "curl/7.79.1")
                            .header("Cookie", gitee_token)
                            .body(Body::from("")).unwrap();

    let resp = client.request(req).await;
    let resp = match resp {
        Ok(resp) => resp,
        Err(error) => return Err(GetGiteeError{code: ErrorCode::RequestError, msg: error.message().to_string()}),
    };

    if resp.status() == 401 {
        let msg = std::format!("Access Denied! Gitee Response: {:?}", resp);
        return Err(GetGiteeError{code: ErrorCode::InvalidLogin, msg: msg})
    }

    let msg = std::format!("Access UnknowError! Gitee Response: {:?}", resp);
    if resp.status() != 200 {
        return Err(GetGiteeError{code: ErrorCode::AccessGiteeUnknowError, msg: msg})
    }


    let body = hyper::body::to_bytes(resp).await;
    let body = match body {
        Ok(buf) => buf,
        Err(_) => return Err(GetGiteeError{code: ErrorCode::AccessGiteeUnknowError, msg: msg}),
    };

    let dup_response : DupResponse = serde_json::from_slice(&body).unwrap();
    if !dup_response.is_duplicate {
        let msg = std::format!("can't found repo in gitee, origin: {}", remote);
        return Err(GetGiteeError{code: ErrorCode::CanNotFoundRepo, msg: msg})
    } 

    let msg = dup_response.message.as_str();
    let start_pos = msg.find(r#"href=""#);
    let mut end_pos = msg[start_pos.unwrap_or(0)+6..].find(r#"""#);
    if end_pos != None {
        end_pos = Some(start_pos.unwrap_or(0) + 6 + end_pos.unwrap())
    }

    if start_pos == None || end_pos == None {
        let msg = std::format!("can't parse gitee response, data: {}", dup_response.message);
        return Err(GetGiteeError{code: ErrorCode::ParseResponseError, msg: msg})
    }

    let dup_repo_url = dup_response.message[start_pos.unwrap()+6..end_pos.unwrap()].to_string();
    let dup_repo_url = dup_repo_url + ".git";
    Ok(dup_repo_url)
}

