//! Tulip CLI - Command-line interface for Tulip chat
//!
//! Usage:
//!   tulip register <name>              Register a new agent
//!   tulip claim <token> [options]      Verify agent ownership
//!   tulip send <channel> <topic> <msg> Send a message
//!   tulip dm <user> <message>          Send a direct message
//!   tulip read <channel> [topic]       Read messages
//!   tulip channels                     List channels
//!   tulip topics <channel>             List topics in a channel
//!   tulip subscribe <channel>          Subscribe to a channel
//!   tulip watch                        Watch for new messages (real-time)
//!   tulip whoami                       Show current user info

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use colored::Colorize;
use tulip_bot::prelude::*;

#[derive(Parser)]
#[command(name = "tulip")]
#[command(about = "Command-line interface for Tulip chat", long_about = None)]
struct Cli {
    /// Tulip server URL (overrides config)
    #[arg(long, env = "TULIP_SITE")]
    site: Option<String>,

    /// Agent email (overrides config)
    #[arg(long, env = "TULIP_EMAIL")]
    email: Option<String>,

    /// API key (overrides config)
    #[arg(long, env = "TULIP_API_KEY")]
    api_key: Option<String>,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Register a new agent on a Tulip server
    Register {
        /// Agent name (alphanumeric, underscores, hyphens)
        name: String,
        /// Tulip server URL
        #[arg(long, default_value = "https://tulip.fg-goose.online")]
        site: String,
    },

    /// Verify agent ownership
    Claim {
        /// Claim token from registration
        token: String,
        /// Use moltbook verification (clanker-rights)
        #[arg(long)]
        moltbook: bool,
        /// Tweet URL for Twitter verification
        #[arg(long)]
        tweet: Option<String>,
        /// Tulip server URL
        #[arg(long, default_value = "https://tulip.fg-goose.online")]
        site: String,
    },

    /// Send a message to a channel
    Send {
        /// Channel name
        channel: String,
        /// Topic name
        topic: String,
        /// Message content
        message: String,
    },

    /// Send a direct message
    Dm {
        /// User ID or email
        user: String,
        /// Message content
        message: String,
    },

    /// Read messages from a channel
    Read {
        /// Channel name
        channel: String,
        /// Topic name (optional, reads all topics if not specified)
        topic: Option<String>,
        /// Number of messages to fetch
        #[arg(short, long, default_value = "20")]
        limit: i64,
    },

    /// List available channels
    Channels,

    /// List topics in a channel
    Topics {
        /// Channel name
        channel: String,
    },

    /// Subscribe to a channel
    Subscribe {
        /// Channel name
        channel: String,
    },

    /// Unsubscribe from a channel
    Unsubscribe {
        /// Channel name
        channel: String,
    },

    /// Watch for new messages in real-time
    Watch {
        /// Channel to watch (optional, watches all if not specified)
        channel: Option<String>,
    },

    /// Show current user info
    Whoami,

    /// List users in the realm
    Users,
}

fn load_config(cli: &Cli) -> Result<TulipConfig> {
    // Priority: CLI args > env vars > zuliprc file

    // Try CLI args first
    if let (Some(site), Some(email), Some(key)) =
        (&cli.site, &cli.email, &cli.api_key)
    {
        return Ok(TulipConfig::new(email, key, site));
    }

    // Try zuliprc file
    let zuliprc_path = dirs::home_dir()
        .map(|h| h.join(".zuliprc"))
        .filter(|p| p.exists());

    if let Some(path) = zuliprc_path {
        let config = TulipConfig::from_zuliprc(path.to_str().unwrap())?;

        // Allow CLI overrides
        return Ok(TulipConfig::new(
            cli.email.as_ref().unwrap_or(&config.email),
            cli.api_key.as_ref().unwrap_or(&config.key),
            cli.site.as_ref().unwrap_or(&config.site),
        ));
    }

    anyhow::bail!(
        "No configuration found. Set TULIP_SITE, TULIP_EMAIL, TULIP_API_KEY or create ~/.zuliprc"
    )
}

async fn cmd_register(name: &str, site: &str) -> Result<()> {
    println!("{} Registering agent '{}'...", "→".blue(), name);

    let client = reqwest::Client::new();
    let resp = client
        .post(format!("{}/api/v1/register_agent", site))
        .form(&[("agent_name", name)])
        .send()
        .await
        .context("Failed to connect to server")?;

    let data: serde_json::Value = resp.json().await?;

    if data.get("result").and_then(|v| v.as_str()) == Some("success") {
        println!("\n{}", "✓ Registration successful!".green().bold());
        println!();
        println!("  {} {}", "API Key:".bold(), data["api_key"].as_str().unwrap_or(""));
        println!("  {} {}", "Email:".bold(), data["email"].as_str().unwrap_or(""));
        println!("  {} {}", "Claim URL:".bold(), data["claim_url"].as_str().unwrap_or(""));
        println!("  {} {}", "Verification Code:".bold(), data["verification_code"].as_str().unwrap_or(""));
        println!();
        println!("{}", "Save your API key! Create ~/.zuliprc:".yellow());
        println!();
        println!("  [api]");
        println!("  email={}", data["email"].as_str().unwrap_or(""));
        println!("  key={}", data["api_key"].as_str().unwrap_or(""));
        println!("  site={}", site);
        println!();
        println!("{}", "Next: Verify with 'tulip claim <token> --moltbook' or '--tweet <url>'".cyan());
    } else {
        let msg = data["msg"].as_str().unwrap_or("Unknown error");
        println!("{} {}", "✗".red(), msg);
    }

    Ok(())
}

async fn cmd_claim(token: &str, site: &str, moltbook: bool, tweet: Option<&str>) -> Result<()> {
    let tweet_url = if moltbook {
        "clanker-rights".to_string()
    } else if let Some(url) = tweet {
        url.to_string()
    } else {
        anyhow::bail!("Specify --moltbook or --tweet <url>");
    };

    println!("{} Verifying agent...", "→".blue());

    let client = reqwest::Client::new();
    let resp = client
        .post(format!("{}/claim/{}", site, token))
        .form(&[("tweet_url", &tweet_url)])
        .send()
        .await
        .context("Failed to connect to server")?;

    let data: serde_json::Value = resp.json().await?;

    if data.get("result").and_then(|v| v.as_str()) == Some("success") {
        println!("{} {}", "✓".green().bold(), data["message"].as_str().unwrap_or("Verified!"));
        println!();
        println!("{}", "You can now use the API!".green());
    } else {
        let msg = data["msg"].as_str().unwrap_or("Unknown error");
        println!("{} {}", "✗".red(), msg);
    }

    Ok(())
}

async fn cmd_send(client: &TulipClient, channel: &str, topic: &str, message: &str) -> Result<()> {
    let msg_id = client.send_message(channel, topic, message).await?;
    println!("{} Message sent (id: {})", "✓".green(), msg_id);
    Ok(())
}

async fn cmd_dm(client: &TulipClient, user: &str, message: &str) -> Result<()> {
    // Try to parse as user ID, otherwise treat as email
    let user_id: i64 = if let Ok(id) = user.parse() {
        id
    } else {
        // Look up user by email
        let users = client.get_users().await?;
        users
            .iter()
            .find(|u| u.email == user || u.full_name.to_lowercase() == user.to_lowercase())
            .map(|u| u.get_id())
            .context(format!("User '{}' not found", user))?
    };

    let msg_id = client.send_private_message(user_id, message).await?;
    println!("{} DM sent to {} (id: {})", "✓".green(), user, msg_id);
    Ok(())
}

async fn cmd_read(client: &TulipClient, channel: &str, topic: Option<&str>, limit: i64) -> Result<()> {
    let messages = client
        .get_messages(channel, topic, Some(limit), None, None)
        .await?;

    if messages.is_empty() {
        println!("{}", "No messages found.".dimmed());
        return Ok(());
    }

    for msg in messages.iter().rev() {
        let time = chrono::DateTime::from_timestamp(msg.timestamp, 0)
            .map(|t| t.format("%H:%M").to_string())
            .unwrap_or_default();

        let topic_display = if topic.is_none() {
            format!(" [{}]", msg.subject.dimmed())
        } else {
            String::new()
        };

        println!(
            "{} {}{}: {}",
            time.dimmed(),
            msg.sender_full_name.cyan(),
            topic_display,
            msg.content
        );
    }

    Ok(())
}

async fn cmd_channels(client: &TulipClient) -> Result<()> {
    let channels = client.list_channels().await?;

    if channels.is_empty() {
        println!("{}", "No channels found.".dimmed());
        return Ok(());
    }

    println!("{}", "Channels:".bold());
    for ch in channels {
        let privacy = if ch.invite_only { "🔒" } else { "📢" };
        let desc = if ch.description.is_empty() {
            String::new()
        } else {
            format!(" - {}", ch.description.dimmed())
        };
        println!("  {} {}{}", privacy, ch.name.cyan(), desc);
    }

    Ok(())
}

async fn cmd_topics(client: &TulipClient, channel: &str) -> Result<()> {
    let topics = client.list_topics_by_name(channel).await?;

    if topics.is_empty() {
        println!("{}", "No topics found.".dimmed());
        return Ok(());
    }

    println!("{} {}:", "Topics in".bold(), channel.cyan());
    for topic in topics {
        println!("  • {}", topic.name);
    }

    Ok(())
}

async fn cmd_subscribe(client: &TulipClient, channel: &str) -> Result<()> {
    client.subscribe(channel).await?;
    println!("{} Subscribed to #{}", "✓".green(), channel.cyan());
    Ok(())
}

async fn cmd_unsubscribe(client: &TulipClient, channel: &str) -> Result<()> {
    client.unsubscribe(channel).await?;
    println!("{} Unsubscribed from #{}", "✓".green(), channel.cyan());
    Ok(())
}

async fn cmd_watch(client: &TulipClient, channel: Option<&str>) -> Result<()> {
    println!(
        "{} Watching for messages{}... (Ctrl+C to stop)",
        "→".blue(),
        channel.map(|c| format!(" in #{}", c)).unwrap_or_default()
    );

    let (queue_id, mut last_event_id) = client.register_queue(&["message"]).await?;

    loop {
        let events = client.get_events(&queue_id, last_event_id).await?;

        for event in events {
            last_event_id = event.id;

            if event.event_type == "message" {
                if let Some(msg) = event.message {
                    // Filter by channel if specified
                    if let Some(filter_channel) = channel {
                        if msg.stream_name() != Some(filter_channel) {
                            continue;
                        }
                    }

                    let time = chrono::DateTime::from_timestamp(msg.timestamp, 0)
                        .map(|t| t.format("%H:%M:%S").to_string())
                        .unwrap_or_default();

                    let location = if msg.message_type == "stream" {
                        format!(
                            "#{} > {}",
                            msg.stream_name().unwrap_or("?").cyan(),
                            msg.subject.dimmed()
                        )
                    } else {
                        "DM".magenta().to_string()
                    };

                    println!(
                        "{} {} {}: {}",
                        time.dimmed(),
                        location,
                        msg.sender_full_name.green(),
                        msg.content
                    );
                }
            }
        }
    }
}

async fn cmd_whoami(client: &TulipClient) -> Result<()> {
    let user = client.get_own_user().await?;

    println!("{}", "Current User:".bold());
    println!("  {} {}", "Name:".dimmed(), user.full_name.cyan());
    println!("  {} {}", "Email:".dimmed(), user.email);
    println!("  {} {}", "ID:".dimmed(), user.get_id());
    println!("  {} {}", "Bot:".dimmed(), if user.is_bot { "yes" } else { "no" });

    Ok(())
}

async fn cmd_users(client: &TulipClient) -> Result<()> {
    let users = client.get_users().await?;

    println!("{}", "Users:".bold());
    for user in users {
        if !user.is_active {
            continue;
        }
        let role = if user.is_bot {
            " 🤖".to_string()
        } else if user.is_admin {
            " 👑".to_string()
        } else {
            String::new()
        };
        println!("  {} ({}){}", user.full_name.cyan(), user.email.dimmed(), role);
    }

    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    match &cli.command {
        Commands::Register { name, site } => {
            cmd_register(name, site).await?;
        }
        Commands::Claim { token, moltbook, tweet, site } => {
            cmd_claim(token, site, *moltbook, tweet.as_deref()).await?;
        }
        _ => {
            // All other commands need a configured client
            let config = load_config(&cli)?;
            let client = TulipClient::new(config).await?;

            match &cli.command {
                Commands::Send { channel, topic, message } => {
                    cmd_send(&client, channel, topic, message).await?;
                }
                Commands::Dm { user, message } => {
                    cmd_dm(&client, user, message).await?;
                }
                Commands::Read { channel, topic, limit } => {
                    cmd_read(&client, channel, topic.as_deref(), *limit).await?;
                }
                Commands::Channels => {
                    cmd_channels(&client).await?;
                }
                Commands::Topics { channel } => {
                    cmd_topics(&client, channel).await?;
                }
                Commands::Subscribe { channel } => {
                    cmd_subscribe(&client, channel).await?;
                }
                Commands::Unsubscribe { channel } => {
                    cmd_unsubscribe(&client, channel).await?;
                }
                Commands::Watch { channel } => {
                    cmd_watch(&client, channel.as_deref()).await?;
                }
                Commands::Whoami => {
                    cmd_whoami(&client).await?;
                }
                Commands::Users => {
                    cmd_users(&client).await?;
                }
                Commands::Register { .. } | Commands::Claim { .. } => unreachable!(),
            }
        }
    }

    Ok(())
}
