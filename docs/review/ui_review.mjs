/* global window, localStorage, setTimeout, WebSocket, fetch, console, URL, Buffer */
// Isolated browser review of actual React UI with a synthetic Tauri bridge.
// Requires the existing Vite preview at http://127.0.0.1:1420/ and installed Edge.
import fs from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import {spawn} from 'node:child_process';
const profile=await fs.mkdtemp(path.join(os.tmpdir(),'tz-review-edge-'));
const browser=spawn('C:/Program Files (x86)/Microsoft/Edge/Application/msedge.exe',['--headless=new','--disable-gpu','--no-first-run','--remote-debugging-port=0',`--user-data-dir=${profile}`,'about:blank'],{windowsHide:true,stdio:'ignore'});
const sleep=ms=>new Promise(r=>setTimeout(r,ms));
let port;for(let i=0;i<80;i++){try{port=(await fs.readFile(path.join(profile,'DevToolsActivePort'),'utf8')).split('\n')[0];break;}catch{await sleep(100);}}
if(!port)throw Error('Browser debug endpoint unavailable');
const targets=await (await fetch(`http://127.0.0.1:${port}/json/list`)).json();
const ws=new WebSocket(targets.find(t=>t.type==='page').webSocketDebuggerUrl);
await new Promise(r=>ws.addEventListener('open',r,{once:true}));
let seq=0;const pending=new Map();
ws.addEventListener('message',e=>{const m=JSON.parse(e.data);if(m.id){const p=pending.get(m.id);pending.delete(m.id);if(m.error){p.reject(m.error);}else{p.resolve(m.result);}}});
const call=(method,params={})=>new Promise((resolve,reject)=>{const id=++seq;pending.set(id,{resolve,reject});ws.send(JSON.stringify({id,method,params}));});
const evaluate=async expression=>{const r=await call('Runtime.evaluate',{expression,awaitPromise:true,returnByValue:true});if(r.exceptionDetails)throw Error(JSON.stringify(r.exceptionDetails));return r.result.value;};
const init=()=>{
  const names={alpha:'Alpha',beta:'Beta'};const character=id=>({schema_version:1,id,name:names[id],summary:'Synthetic review character',system_prompt:'You are a fictional character.',traits:[],boundaries:[],tags:[]});
  const turn=(id,role,content)=>({id,timestamp:new Date().toISOString(),role,status:'complete',content});
  const transcript=(id,session=`session-${id}`,turns=[turn(`welcome-${id}`,'assistant',`Welcome from ${names[id]}.`)])=>({schema_version:1,character_id:id,session_id:session,title:`Chat with ${names[id]}`,created_at:'',updated_at:'',turns});
  const provider={id:'review',kind:'ollama',endpoint:'http://127.0.0.1:11434',chat_model:'llama3.2:latest',embedding_model:null,bearer_token:null};
  const context={retrieval_mode:'lexical',fallback_reason:'No embedding model configured',estimated_input_tokens:100,input_token_limit:3000,reserved_output_tokens:1000,omitted_turns:0,selected_memory_tokens:0,candidate_memories:0,omitted_memories:0};
  let callbackId=0;const callbacks=new Map();
  window.review={calls:[],pending:[],transcript};
  window.__TAURI_EVENT_PLUGIN_INTERNALS__={unregisterListener:()=>{}};
  window.__TAURI_INTERNALS__={transformCallback:cb=>{callbacks.set(++callbackId,cb);return callbackId;},unregisterCallback:id=>callbacks.delete(id),invoke:async(cmd,args={})=>{
    window.review.calls.push({cmd,args:JSON.parse(JSON.stringify(args))});const id=args.vaultRoot==='beta'?'beta':'alpha';
    if(cmd.startsWith('plugin:'))return 1;
    if(cmd==='app_info')return{name:'tz-chatter',version:'0.1.0',stage:'review'};
    if(cmd==='load_provider_settings')return{schema_version:1,active_provider_id:'review',providers:[provider]};
    if(cmd==='provider_discover')return{provider_id:'review',models:[{id:'llama3.2:latest',supports_chat:true,supports_embeddings:false}]};
    if(cmd==='provider_health')return{provider_id:'review',reachable:true};
    if(cmd==='character_library_list')return{last_parent_dir:'',entries:Object.keys(names).map(id=>({vault_root:id,character_id:id,name:names[id],error:null,has_portrait:false}))};
    if(cmd==='character_load')return character(id);
    if(cmd==='character_portrait_load')return null;
    if(cmd==='persona_load')return{schema_version:1,body:''};
    if(cmd==='generation_load')return{schema_version:1};
    if(cmd==='locals_list')return[];
    if(cmd==='scene_settings_load')return{schema_version:1,default_local:null};
    if(cmd==='conversation_resume')return{character:character(id),transcript:transcript(id)};
    if(cmd==='conversation_start_session')return{character:character(id),transcript:transcript(id,`new-${id}`,[])};
    if(cmd==='conversation_list_sessions')return[{session_id:`session-${id}`,title:`Chat with ${names[id]}`,created_at:'',updated_at:'',turn_count:1,preview:'Welcome',archived:false}];
    if(cmd==='initiative_snapshot')return{settings:{enabled:false},state:{},decision:{eligible:false}};
    if(cmd==='initiative_scheduler_stop'||cmd==='conversation_cancel')return true;
    if(cmd==='memory_browse'||cmd==='memory_review_queue'||cmd==='memory_conflict_pairs')return[];
    if(cmd==='conversation_send')return new Promise(resolve=>{
      const s=args.snapshot;const u=turn(s.user_turn_id,'user',s.user_content);const a=turn(`reply-${s.user_turn_id}`,'assistant',`Late reply from ${s.character.name}`);
      args.onEvent.onmessage({event:'started',data:{cancellation_id:a.id}});
      window.review.pending.push(()=>resolve({transcript:transcript(s.character.id,s.session_id,[u,a]),assistant_turn:a,retrieved_memories:[],context_inspection:context}));
    });
    return null;
  }};
  localStorage.setItem('tz-chatter.vault-root','alpha');localStorage.setItem('tz-chatter.active-character-id','alpha');localStorage.setItem('tz-chatter.active-character-name','Alpha');localStorage.setItem('tz-chatter.active-session-id','session-alpha');
};
await call('Page.enable');await call('Page.addScriptToEvaluateOnNewDocument',{source:`(${init.toString()})()`});
await call('Emulation.setDeviceMetricsOverride',{width:1280,height:800,deviceScaleFactor:1,mobile:false});
await call('Page.navigate',{url:'http://127.0.0.1:1420/'});await sleep(1400);
const globalResult=await call('Runtime.evaluate',{expression:'globalThis'});
const globalObjectId=globalResult.result.objectId;
if(!globalObjectId)throw Error('Browser global object unavailable');
const callPageFunction=async(functionDeclaration,values=[])=>{
  const r=await call('Runtime.callFunctionOn',{
    functionDeclaration,
    objectId:globalObjectId,
    arguments:values.map(value=>({value})),
    awaitPromise:true,
    returnByValue:true,
  });
  if(r.exceptionDetails)throw Error(JSON.stringify(r.exceptionDetails));
  return r.result.value;
};
const screenshot=async name=>{const r=await call('Page.captureScreenshot',{format:'png'});await fs.writeFile(new URL(name,import.meta.url),Buffer.from(r.data,'base64'));};
const clickButton=label=>callPageFunction(
  `function(label){const button=[...document.querySelectorAll('button')].find(candidate=>candidate.textContent.trim().endsWith(label));if(!button)throw Error('Button not found');button.click();}`,
  [label],
);
const input=text=>callPageFunction(
  `function(text){const el=document.querySelector('textarea[aria-label="Message"]');if(!el)throw Error('Message input not found');Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype,'value').set.call(el,text);el.dispatchEvent(new Event('input',{bubbles:true}));}`,
  [text],
);
const results={};
await screenshot('chat-1280.png');
await input('Original message to Alpha');await sleep(50);await clickButton('Send');await sleep(75);
await input('Next draft typed while waiting');await sleep(50);await evaluate('window.review.pending.shift()()');await sleep(100);
results.draftAfterCompletion=await evaluate(`document.querySelector('textarea[aria-label="Message"]').value`);
await input('Second message to Alpha');await sleep(50);await clickButton('Send');await sleep(75);
await evaluate(`window.dispatchEvent(new CustomEvent('tz-chatter-load-vault',{detail:'beta'}))`);await sleep(150);
await evaluate('window.review.pending.shift()()');await sleep(150);
results.characterSwitch=await evaluate(`({active:localStorage.getItem('tz-chatter.active-character-name'),body:document.querySelector('.chat-transcript')?.innerText??document.querySelector('.conversation-panel')?.innerText??document.querySelector('main').innerText,cancelCalls:window.review.calls.filter(c=>c.cmd==='conversation_cancel').length})`);
await screenshot('switched-character-stale-reply.png');
await clickButton('Settings');await sleep(250);
results.settingsFocus=await evaluate(`({focused:document.activeElement?.tagName,insideDialog:!!document.activeElement?.closest('[role="dialog"]')})`);
await call('Input.dispatchKeyEvent',{type:'keyDown',key:'Tab',code:'Tab',windowsVirtualKeyCode:9});await call('Input.dispatchKeyEvent',{type:'keyUp',key:'Tab',code:'Tab',windowsVirtualKeyCode:9});results.settingsTab=await evaluate('({insideDialog:!!document.activeElement?.closest(\'[role=dialog]\'),focused:document.activeElement?.getAttribute(\'aria-label\')})');
await screenshot('settings-1280.png');
await call('Emulation.setDeviceMetricsOverride',{width:900,height:620,deviceScaleFactor:1,mobile:false});await screenshot('settings-900.png');
await clickButton('Done');await sleep(50);await screenshot('chat-900.png');
await clickButton('Characters');await sleep(350);await screenshot('characters-900.png');results.characterLibrary=await evaluate('({height:document.querySelector(\'.character-list\').getBoundingClientRect().height,scrollHeight:document.querySelector(\'.character-list\').scrollHeight})');
await clickButton('Memories');await sleep(150);await screenshot('memories-900.png');
await clickButton('Chat');await sleep(50);
await input('IME pending text');await sleep(50);
await evaluate(`document.querySelector('textarea[aria-label="Message"]').dispatchEvent(new KeyboardEvent('keydown',{key:'Enter',code:'Enter',isComposing:true,bubbles:true,cancelable:true}))`);await sleep(100);
results.imeEnterSubmitted=await evaluate('window.review.pending.length>0');
await fs.writeFile(new URL('ui-results.json',import.meta.url),JSON.stringify(results,null,2));console.log(JSON.stringify(results,null,2));
await call('Browser.close').catch(()=>{});ws.close();browser.unref();
