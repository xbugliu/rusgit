
use std::env::{self};
use std::str::FromStr;
use urlencoding::encode;
use clap::{Parser, Subcommand};
use hyper::{Client, Uri, Request, Method, Body};
use hyper_tls::HttpsConnector;
use std::process::Command;
use serde::{Serialize, Deserialize};

#[derive(Parser)]
#[clap(name = "rusgit")]
#[clap(about = "Pull Github Code From Gitee", long_about = None)]
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
}

#[tokio::main]
async  fn main() {
    let args = Cli::parse();
    
    match &args.command {
        Commands::Clone { remote } => {
            let res = clone(remote).await;
            match res {
                Ok(_) => (),
                Err(err) => print!("{}", err.msg)
            }
        }
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

#[derive(Serialize, Deserialize, Debug)]
struct DupResponse {
    is_duplicate: bool,
    #[serde(default)]
    message: String
}