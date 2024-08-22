use bitpart_interpreter::csml::{load_flows, CsmlInterpreter};
use bitpart_interpreter::Event;
use clap::Parser;
use pgmq::{errors::PgmqError, Message, PGMQueue};

/// The Bitpart interpreter
#[derive(Debug, Parser)]
#[command(version, about, long_about = None)]
struct Args {
    /// Connection URI for postgres database
    #[arg(short, long)]
    connect: String,

    /// Directory of CSML files
    #[arg(short, long)]
    directory: String,
}

////////////////////////////////////////////////////////////////////////////////
// PUBLIC FUNCTION
////////////////////////////////////////////////////////////////////////////////

#[tokio::main]
async fn main() -> Result<(), PgmqError> {
    let args = Args::parse();

    println!("{}", args.connect);
    println!("{}", args.directory);

    let flows = load_flows(&args.directory).unwrap();
    let interpreter = CsmlInterpreter::new("BotId", "BotName", flows);
    interpreter.validate();

    // Initialize a connection to Postgres
    println!("Connecting to Postgres");
    let queue: PGMQueue = PGMQueue::new(args.connect)
        .await
        .expect("Failed to connect to postgres");

    // Create a queue
    println!("Creating a queue 'my_queue'");
    let my_queue = "my_example_queue".to_owned();
    queue
        .create(&my_queue)
        .await
        .expect("Failed to create queue");

    let message = Event {
        content_type: "text".to_owned(),
        content: serde_json::Value::String("test".to_owned()),
    };

    // Send the message
    let message_id: i64 = queue
        .send(&my_queue, &message)
        .await
        .expect("Failed to enqueue message");

    // Use a visibility timeout of 30 seconds
    // Once read, the message will be unable to be read
    // until the visibility timeout expires
    let visibility_timeout_seconds: i32 = 30;

    // Read a message
    let received_message: Message<Event> = queue
        .read::<Event>(&my_queue, Some(visibility_timeout_seconds))
        .await
        .unwrap()
        .expect("No messages in the queue");
    println!("Received a message: {:?}", received_message);

    assert_eq!(received_message.msg_id, message_id);

    // archive the messages
    let _ = queue
        .archive(&my_queue, received_message.msg_id)
        .await
        .expect("Failed to archive message");
    println!("archived the messages from the queue");

    Ok(())
}
