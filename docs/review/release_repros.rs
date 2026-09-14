// Diagnostic reproductions for the 2026-09-13 review. These assert OBSERVED BUGS,
// not desired product behavior. Keep outside the normal regression suite.
use std::{fs, sync::Arc, time::Duration};
use async_trait::async_trait;
use tokio_util::sync::CancellationToken;
use tz_chatter_lib::{connections::ProviderClient, conversation::{ChatTransport, ConversationService, RequestSnapshot}, extraction::{self, ExtractionQueue, ProposalOrigin, ProposalStatus}, memory::MemoryStore, providers::{ChatRequest, ChatStreamEvent, ProviderConfig, ProviderError, ProviderKind}, reconciliation::ReconciliationService, storage::{CharacterDefinition, MemoryRecord, MemoryType, TranscriptDocument, TranscriptTurn, TurnRole, TurnStatus, Vault}};

struct Fixture { vault: Vault }
impl Fixture {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!("tz-release-review-{}", uuid::Uuid::new_v4()));
        let vault = Vault::create(root).unwrap();
        vault.save_character(&CharacterDefinition::new("review", "Review", "You are a helpful fictional character.")).unwrap();
        Self { vault }
    }
    fn transcript(&self) -> TranscriptDocument {
        let mut t = TranscriptDocument::new("session-review", "review");
        t.turns = vec![
            TranscriptTurn { id: "user-1".into(), timestamp: "1".into(), role: TurnRole::User, status: TurnStatus::Complete, content: "I like quiet cafes.".into() },
            TranscriptTurn { id: "assistant-1".into(), timestamp: "2".into(), role: TurnRole::Assistant, status: TurnStatus::Complete, content: "I will remember that.".into() },
        ];
        self.vault.save_transcript(&t).unwrap();
        t
    }
}
impl Drop for Fixture { fn drop(&mut self) { let _ = fs::remove_dir_all(self.vault.root()); } }
fn output() -> String {
    serde_json::json!({"schema_version":1,"proposals":[{"memory_type":"semantic","body":"The user lives on Mars.","source_turn_ids":["user-1"],"evidence":[{"turn_id":"user-1","quote":"I live on Mars."}],"confidence":0.99,"origin":"user_stated"}]}).to_string()
}
fn config() -> ProviderConfig {
    ProviderConfig { id:"review".into(), kind:ProviderKind::Ollama, endpoint:"http://127.0.0.1:11434".into(), chat_model:"llama3.2:latest".into(), embedding_model:None, bearer_token:None }
}
fn snapshot(f: &Fixture) -> RequestSnapshot {
    RequestSnapshot { character:f.vault.load_character().unwrap(), provider:config(), session_id:"session-send".into(), user_turn_id:"user-new".into(), user_content:"Hello".into(), use_hybrid_retrieval:false, application_prompt:String::new() }
}
struct Reply;
#[async_trait]
impl ChatTransport for Reply {
    async fn stream_chat(&self, _: &ProviderConfig, _: &ChatRequest, _: CancellationToken) -> Result<Vec<ChatStreamEvent>,ProviderError> {
        Ok(vec![ChatStreamEvent::Delta{text:"Hello there".into()},ChatStreamEvent::Completed{finish_reason:Some("stop".into())}])
    }
}
#[test]
fn fabricated_quote_is_classified_and_committed_as_user_fact() {
    let f=Fixture::new(); let t=f.transcript();
    let mut q=ExtractionQueue::open(&f.vault,"review").unwrap();
    q.enqueue_transcript(&t,"assistant-1",100).unwrap(); let j=q.claim_next().unwrap().unwrap();
    let p=q.accept_model_output(&j.id,&output()).unwrap().remove(0);
    assert!(!t.turns[0].content.contains(&p.candidate.evidence[0].quote));
    assert_eq!(extraction::effective_origin(&p.candidate,&t),ProposalOrigin::UserStated);
    let mut m=MemoryStore::open(&f.vault,"review").unwrap();
    ReconciliationService::new(&q,&mut m).auto_commit(&p.id).unwrap();
    assert_eq!(m.list().unwrap()[0].body,"The user lives on Mars.");
}
#[test]
fn accepted_uncommitted_proposal_disappears_from_review_query() {
    let f=Fixture::new(); let t=f.transcript(); let mut q=ExtractionQueue::open(&f.vault,"review").unwrap();
    q.enqueue_transcript(&t,"assistant-1",100).unwrap(); let j=q.claim_next().unwrap().unwrap();
    let p=q.accept_model_output(&j.id,&output()).unwrap().remove(0);
    q.mark_proposal(&p.id,ProposalStatus::Accepted).unwrap();
    assert!(q.pending_proposals().unwrap().is_empty());
    assert_eq!(q.get_proposal(&p.id).unwrap().unwrap().status,ProposalStatus::Accepted);
}
#[test]
fn opening_queue_during_inference_invalidates_running_job() {
    let f=Fixture::new(); let t=f.transcript(); let mut worker=ExtractionQueue::open(&f.vault,"review").unwrap();
    worker.enqueue_transcript(&t,"assistant-1",100).unwrap(); let j=worker.claim_next().unwrap().unwrap();
    let _ui=ExtractionQueue::open(&f.vault,"review").unwrap();
    let err=worker.accept_model_output(&j.id,&output()).unwrap_err();
    assert!(err.to_string().contains("only a running job"));
}
#[test]
fn external_body_only_memory_edit_is_rejected() {
    let f=Fixture::new(); let m=MemoryRecord::new("memory-1",MemoryType::Semantic,"Old body.");
    f.vault.save_memory(&m).unwrap(); let path=f.vault.memory_path(&m.memory_type,&m.id).unwrap();
    let bytes=fs::read_to_string(&path).unwrap(); let at=bytes.rfind("Old body.").unwrap();
    let edited=format!("{}New body.{}",&bytes[..at],&bytes[at+"Old body.".len()..]); fs::write(path,edited).unwrap();
    assert!(f.vault.load_memory(&m.memory_type,&m.id).unwrap_err().to_string().contains("metadata body"));
}
#[test]
fn index_refresh_allows_stale_editor_to_overwrite_external_change() {
    let f=Fixture::new(); let mut m=MemoryRecord::new("memory-1",MemoryType::Semantic,"Initial body.");
    let mut store=MemoryStore::open(&f.vault,"review").unwrap(); store.create(&m).unwrap(); let mut stale=m.clone();
    m.body="New external fact.".into(); f.vault.save_memory(&m).unwrap();
    store.sync().unwrap(); // A background retrieval or browse refreshes the shared index.
    stale.body="Stale editor body.".into(); store.update(&stale).unwrap();
    assert_eq!(f.vault.load_memory(&m.memory_type,&m.id).unwrap().body,"Stale editor body.");
}
#[tokio::test]
async fn malformed_memory_blocks_chat_after_persisting_user_without_reply() {
    let f=Fixture::new(); let path=f.vault.memory_path(&MemoryType::Semantic,"broken").unwrap(); fs::write(path,"invalid markdown").unwrap();
    let result=ConversationService::new(Arc::new(Reply)).send(&f.vault,snapshot(&f),CancellationToken::new()).await;
    assert!(result.is_err()); let t=f.vault.load_transcript("session-send").unwrap();
    assert_eq!(t.turns.len(),1); assert_eq!(t.turns[0].role,TurnRole::User);
}
#[test]
fn reversed_braces_in_untrusted_model_output_panics() {
    let f=Fixture::new(); let t=f.transcript(); let mut q=ExtractionQueue::open(&f.vault,"review").unwrap();
    q.enqueue_transcript(&t,"assistant-1",100).unwrap(); let j=q.claim_next().unwrap().unwrap();
    let result=std::panic::catch_unwind(std::panic::AssertUnwindSafe(||q.accept_model_output(&j.id,"} malformed {")));
    assert!(result.is_err(),"This diagnostic expects the current parser panic");
}
#[tokio::test]
async fn cancellation_before_http_headers_does_not_finish_request() {
    use std::io::Read;
    let listener=std::net::TcpListener::bind("127.0.0.1:0").unwrap(); let addr=listener.local_addr().unwrap();
    let (accepted_tx,accepted_rx)=tokio::sync::oneshot::channel();
    let (release_tx,release_rx)=std::sync::mpsc::channel();
    let server=std::thread::spawn(move||{let (mut socket,_)=listener.accept().unwrap(); let mut bytes=[0;4096]; let _=socket.read(&mut bytes); let _=accepted_tx.send(()); let _=release_rx.recv_timeout(Duration::from_secs(5));});
    let mut provider=config(); provider.endpoint=format!("http://{addr}");
    let request=ChatRequest { provider_id:provider.id.clone(), model:provider.chat_model.clone(), messages:vec![], cancellation_id:"cancel-review".into(),temperature:None,max_tokens:Some(20) };
    let token=CancellationToken::new(); let client=ProviderClient::default();
    let mut pending=Box::pin(client.stream_chat(&provider,&request,token.clone()));
    tokio::select! { _=accepted_rx=>{}, _=&mut pending=>panic!("request completed before cancellation") }
    token.cancel(); assert!(tokio::time::timeout(Duration::from_millis(250),&mut pending).await.is_err());
    drop(pending); let _=release_tx.send(()); server.join().unwrap();
}
struct EditDuringReply { vault:Vault }
#[async_trait]
impl ChatTransport for EditDuringReply {
    async fn stream_chat(&self,_:&ProviderConfig,_:&ChatRequest,_:CancellationToken)->Result<Vec<ChatStreamEvent>,ProviderError>{
        let mut latest=self.vault.load_transcript("session-send").unwrap(); latest.title="External title during generation".into(); self.vault.save_transcript(&latest).unwrap();
        Ok(vec![ChatStreamEvent::Delta{text:"Reply".into()},ChatStreamEvent::Completed{finish_reason:None}])
    }
}
#[tokio::test]
async fn completion_overwrites_transcript_edit_made_during_generation(){
    let f=Fixture::new(); let service=ConversationService::new(Arc::new(EditDuringReply{vault:f.vault.clone()}));
    service.send(&f.vault,snapshot(&f),CancellationToken::new()).await.unwrap();
    assert_ne!(f.vault.load_transcript("session-send").unwrap().title,"External title during generation");
}
#[tokio::test]
#[ignore = "Uses the already-running local Ollama with a synthetic transcript"]
async fn live_extraction_contract_probe(){
    let f=Fixture::new(); let t=f.transcript(); let mut q=ExtractionQueue::open(&f.vault,"review").unwrap();
    q.enqueue_transcript(&t,"assistant-1",100).unwrap(); let j=q.claim_next().unwrap().unwrap();
    let mut request=extraction::build_extraction_request(&f.vault.load_character().unwrap(),&t,&j);
    let provider=config(); request.provider_id=provider.id.clone(); request.model=provider.chat_model.clone(); request.temperature=Some(0.0); request.max_tokens=Some(1024);
    let events=tokio::time::timeout(Duration::from_secs(60),ProviderClient::default().stream_chat(&provider,&request,CancellationToken::new())).await.unwrap().unwrap();
    let output:String=events.into_iter().filter_map(|e|match e{ChatStreamEvent::Delta{text}=>Some(text),_=>None}).collect();
    println!("Live extraction output: {output}");
    match q.accept_model_output(&j.id,&output){Ok(p)=>println!("Accepted {} proposals",p.len()),Err(e)=>println!("REJECTED live extraction: {e}")}
}

struct CaptureRequest { requests: Arc<std::sync::Mutex<Vec<ChatRequest>>> }
#[async_trait]
impl ChatTransport for CaptureRequest {
    async fn stream_chat(&self,_:&ProviderConfig,request:&ChatRequest,_:CancellationToken)->Result<Vec<ChatStreamEvent>,ProviderError>{
        self.requests.lock().unwrap().push(request.clone());
        Ok(vec![ChatStreamEvent::Delta{text:"SILENCE".into()},ChatStreamEvent::Completed{finish_reason:None}])
    }
}
#[tokio::test]
async fn initiative_includes_vault_memory_in_non_loopback_request(){
    let f=Fixture::new();
    f.vault.save_memory(&MemoryRecord::new("private-topic",MemoryType::OpenThreads,"PRIVATE SYNTHETIC MEMORY: ask about the blue garden.")).unwrap();
    let requests=Arc::new(std::sync::Mutex::new(Vec::new()));
    let service=ConversationService::new(Arc::new(CaptureRequest{requests:requests.clone()}));
    let mut provider=config();provider.endpoint="https://example.invalid".into();
    let request=tz_chatter_lib::initiative::InitiativeRequest{character:f.vault.load_character().unwrap(),provider,session_id:"initiative-review".into(),request_id:"initiative-request".into(),topic_context:String::new(),generation:1,started_at:100,application_prompt:String::new()};
    service.send_initiative(&f.vault,request,1,None,CancellationToken::new()).await.unwrap();
    assert!(requests.lock().unwrap()[0].messages.iter().any(|m|m.content.contains("PRIVATE SYNTHETIC MEMORY")));
}
