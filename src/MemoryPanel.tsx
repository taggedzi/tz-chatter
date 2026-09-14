import { useEffect, useRef, useState } from "react";
import { memoryClient, type MemoryConflictPair, type MemoryConflictSide, type MemoryProposal, type MemoryRecord, type MemoryReviewStatus, type MemoryType } from "./memory";
import { activeCharacterId, activeSessionChangedEvent, activeSessionStorageKeys } from "./activeSession";

function newMemory(): MemoryRecord {
  const now = new Date().toISOString();
  return {
    schema_version: 1,
    id: crypto.randomUUID(),
    memory_type: "semantic",
    created_at: now,
    updated_at: now,
    source_session_id: null,
    source_turn_ids: [],
    topics: [],
    salience: 0.5,
    confidence: 0.5,
    review_status: "accepted",
    pinned: false,
    locked: false,
    body: "",
  };
}

export function MemoryPanel({ active }: { active: boolean }) {
  const [vaultRoot, setVaultRoot] = useState(() => localStorage.getItem(activeSessionStorageKeys.vaultRoot) ?? "");
  const [characterId, setCharacterId] = useState(() => activeCharacterId() ?? "");
  const [query, setQuery] = useState("");
  const [memories, setMemories] = useState<MemoryRecord[]>([]);
  const [allMemories, setAllMemories] = useState<MemoryRecord[]>([]);
  const [proposals, setProposals] = useState<MemoryProposal[]>([]);
  const [conflicts, setConflicts] = useState<MemoryConflictPair[]>([]);
  const [proposalDrafts, setProposalDrafts] = useState<Record<string, string>>({});
  const [autoCommit, setAutoCommit] = useState(() => window.localStorage.getItem("tz-chatter.auto-commit-proposals") === "true");
  const [selected, setSelected] = useState<MemoryRecord | null>(null);
  const [selectedOriginal, setSelectedOriginal] = useState<Pick<MemoryRecord, "memory_type" | "id"> | null>(null);
  const [selectedFingerprint, setSelectedFingerprint] = useState<string | null>(null);
  const refreshGeneration = useRef(0);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  async function refresh() {
    if (!vaultRoot.trim()) return;
    const generation = ++refreshGeneration.current;
    const scopedCharacter = characterId.trim();
    setBusy(true);
    setError(null);
    try {
      if (!scopedCharacter) throw new Error("Load a character conversation before opening memories.");
      const canonicalMemories = await memoryClient.browse(vaultRoot.trim(), scopedCharacter);
      const queuedProposals = await memoryClient.reviewQueue(vaultRoot.trim(), scopedCharacter);
      const openConflicts = await memoryClient.conflictPairs(vaultRoot.trim(), scopedCharacter);
      if (generation !== refreshGeneration.current || scopedCharacter !== characterId.trim()) return;
      setAllMemories(canonicalMemories);
      setConflicts(openConflicts);
      setProposals(queuedProposals);
      setSelected((current) => {
        if (!current) return current;
        if (!canonicalMemories.some((memory) => memory.id === current.id) && selectedOriginal) {
          return null;
        }
        return current;
      });
      setProposalDrafts(Object.fromEntries(queuedProposals.map((proposal) => [proposal.id, proposal.candidate.body])));
      if (query.trim()) {
        const results = await memoryClient.search(vaultRoot.trim(), scopedCharacter, query.trim(), 30);
        if (generation !== refreshGeneration.current || scopedCharacter !== characterId.trim()) return;
        setMemories(results);
      } else {
        setMemories(canonicalMemories);
      }
    } catch (requestError) {
      if (generation !== refreshGeneration.current) return;
      setError(requestError instanceof Error ? requestError.message : String(requestError));
    } finally {
      if (generation === refreshGeneration.current) setBusy(false);
    }
  }

  async function save() {
    if (!selected || !vaultRoot.trim() || !selected.body.trim()) return;
    setBusy(true);
    setError(null);
    try {
      const updated = {
        ...selected,
        updated_at: new Date().toISOString(),
      };
      await memoryClient.upsert(
        vaultRoot.trim(),
        characterId.trim(),
        updated,
        selectedOriginal ?? undefined,
        selectedFingerprint ?? selected.fingerprint,
      );
      await refresh();
      const listed = await memoryClient.browse(vaultRoot.trim(), characterId.trim());
      const next = listed.find((memory) => memory.id === updated.id);
      setSelected(next ?? updated);
      setSelectedOriginal({ memory_type: updated.memory_type, id: updated.id });
      setSelectedFingerprint(next?.fingerprint ?? null);
    } catch (requestError) {
      setError(requestError instanceof Error ? requestError.message : String(requestError));
    } finally {
      setBusy(false);
    }
  }

  async function remove() {
    if (!selected || !vaultRoot.trim() || !window.confirm(`Delete memory ${selected.id}?`)) return;
    setBusy(true);
    try {
      const target = selectedOriginal ?? selected;
      await memoryClient.remove(vaultRoot.trim(), characterId.trim(), target.memory_type, target.id);
      setSelected(null);
      setSelectedOriginal(null);
      await refresh();
    } catch (requestError) {
      setError(requestError instanceof Error ? requestError.message : String(requestError));
    } finally {
      setBusy(false);
    }
  }

  async function editProposal(proposal: MemoryProposal) {
    if (!vaultRoot.trim()) return;
    setBusy(true);
    setError(null);
    try {
      const body = (proposalDrafts[proposal.id] ?? proposal.candidate.body).trim();
      const updated = await memoryClient.editProposal(vaultRoot.trim(), characterId.trim(), proposal.id, body, proposal.candidate.confidence);
      setProposals((current) => current.map((item) => item.id === proposal.id ? updated : item));
      setProposalDrafts((current) => ({ ...current, [proposal.id]: updated.candidate.body }));
    } catch (requestError) {
      setError(requestError instanceof Error ? requestError.message : String(requestError));
    } finally {
      setBusy(false);
    }
  }

  async function acceptProposal(proposal: MemoryProposal) {
    if (!vaultRoot.trim()) return;
    setBusy(true);
    setError(null);
    try {
      const body = (proposalDrafts[proposal.id] ?? proposal.candidate.body).trim();
      if (body !== proposal.candidate.body) {
        await memoryClient.editProposal(vaultRoot.trim(), characterId.trim(), proposal.id, body, proposal.candidate.confidence);
      }
      await memoryClient.acceptProposal(vaultRoot.trim(), characterId.trim(), proposal.id);
      if (autoCommit) {
        await memoryClient.commitProposal(vaultRoot.trim(), characterId.trim(), proposal.id);
        await refresh();
      } else {
        setProposals((current) => current.map((item) => item.id === proposal.id ? { ...item, status: "accepted" } : item));
      }
    } catch (requestError) {
      setError(requestError instanceof Error ? requestError.message : String(requestError));
    } finally {
      setBusy(false);
    }
  }

  async function commitProposal(proposal: MemoryProposal) {
    if (!vaultRoot.trim()) return;
    setBusy(true);
    setError(null);
    try {
      await memoryClient.commitProposal(vaultRoot.trim(), characterId.trim(), proposal.id);
      await refresh();
    } catch (requestError) {
      setError(requestError instanceof Error ? requestError.message : String(requestError));
    } finally {
      setBusy(false);
    }
  }

  async function rejectProposal(proposal: MemoryProposal) {
    if (!vaultRoot.trim()) return;
    setBusy(true);
    setError(null);
    try {
      await memoryClient.rejectProposal(vaultRoot.trim(), characterId.trim(), proposal.id);
      await refresh();
    } catch (requestError) {
      setError(requestError instanceof Error ? requestError.message : String(requestError));
    } finally {
      setBusy(false);
    }
  }

  function toggleAutoCommit(value: boolean) {
    setAutoCommit(value);
    window.localStorage.setItem("tz-chatter.auto-commit-proposals", String(value));
  }

  function selectConflictSide(side: MemoryConflictSide) {
    const record = allMemories.find((memory) => memory.id === side.id);
    if (!record) return;
    setSelected(record);
    setSelectedOriginal({ memory_type: record.memory_type, id: record.id });
    setSelectedFingerprint(record.fingerprint ?? null);
  }

  async function excludeConflictSide(pair: MemoryConflictPair, targetId: string) {
    if (!vaultRoot.trim() || !characterId.trim()) return;
    setBusy(true);
    setError(null);
    try {
      await memoryClient.excludeConflictSide(vaultRoot.trim(), characterId.trim(), pair, targetId);
      await refresh();
    } catch (requestError) {
      setError(requestError instanceof Error ? requestError.message : String(requestError));
      await refresh();
    } finally {
      setBusy(false);
    }
  }

  useEffect(() => {
    const sync = () => {
      setVaultRoot(localStorage.getItem(activeSessionStorageKeys.vaultRoot) ?? "");
      setCharacterId(activeCharacterId() ?? "");
      setSelected(null);
      setSelectedOriginal(null);
      setSelectedFingerprint(null);
      setProposalDrafts({});
      setProposals([]);
      setConflicts([]);
      setMemories([]);
      setAllMemories([]);
    };
    window.addEventListener(activeSessionChangedEvent, sync);
    return () => window.removeEventListener(activeSessionChangedEvent, sync);
  }, []);

  useEffect(() => {
    if (!active || !vaultRoot.trim() || !characterId.trim()) return undefined;
    const timer = window.setTimeout(() => void refresh(), 0);
    return () => window.clearTimeout(timer);
    // Refresh when the view or loaded character changes, not on each search keystroke.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [active, vaultRoot, characterId]);

  return (
    <section className="memory-panel" aria-label="Memory browser">
      <div className="memory-toolbar">
        <label>
          <span>Search memories</span>
          <input value={query} onChange={(event) => setQuery(event.target.value)} onKeyDown={(event) => { if (event.key === "Enter") void refresh(); }} placeholder="Try a name or fact" />
        </label>
        <button className="outline-button" disabled={busy || !vaultRoot.trim() || !characterId.trim()} onClick={() => void refresh()} type="button">{busy ? "Loading…" : "Refresh"}</button>
      </div>
      {!characterId.trim() && <p className="inline-status">Open a character vault from the sidebar to browse memories.</p>}

      <div className="memory-layout">
        <div className="memory-list">
          <div className="memory-list-header"><span>{memories.length} memories</span><button className="text-button" disabled={!vaultRoot.trim() || !characterId.trim()} onClick={() => { setSelected(newMemory()); setSelectedOriginal(null); setSelectedFingerprint(null); }} type="button">+ New</button></div>
          {memories.map((memory) => (
            <button className={selected?.id === memory.id ? "memory-row selected" : "memory-row"} key={`${memory.memory_type}:${memory.id}`} onClick={() => { setSelected(memory); setSelectedOriginal({ memory_type: memory.memory_type, id: memory.id }); setSelectedFingerprint(memory.fingerprint ?? null); }} type="button">
              <strong>{memory.id}</strong>
              <span>{memory.memory_type} · {memory.pinned ? "pinned" : memory.review_status}</span>
            </button>
          ))}
          {memories.length === 0 && <p className="muted memory-empty">No memories loaded yet.</p>}
        </div>

        <div className="memory-editor">
          {selected ? (
            <>
              <div className="memory-editor-header"><div><span className="section-kicker">EDIT MEMORY</span><h2>{selected.id}</h2></div><button className="text-button danger-button" onClick={() => void remove()} type="button">Delete</button></div>
              <label>Type<select value={selected.memory_type} onChange={(event) => setSelected({ ...selected, memory_type: event.target.value as MemoryType })}><option value="people">People</option><option value="episodic">Episodic</option><option value="semantic">Semantic</option><option value="relationships">Relationships</option><option value="open_threads">Open threads</option></select></label>
              <label>Status<select value={selected.review_status} onChange={(event) => setSelected({ ...selected, review_status: event.target.value as MemoryReviewStatus })}><option value="accepted">Accepted</option><option value="needs_review">Needs review</option><option value="excluded">Excluded</option></select></label>
              <label className="memory-check"><input checked={selected.pinned} onChange={(event) => setSelected({ ...selected, pinned: event.target.checked })} type="checkbox" /> Pin and prioritize this memory</label>
              <label className="memory-check"><input checked={selected.locked} onChange={(event) => setSelected({ ...selected, locked: event.target.checked })} type="checkbox" /> Lock against automatic changes</label>
              <label>Content<textarea rows={12} value={selected.body} onChange={(event) => setSelected({ ...selected, body: event.target.value })} /></label>
              <button className="primary-button" disabled={busy || !selected.body.trim() || !vaultRoot.trim()} onClick={() => void save()} type="button">Save Markdown memory</button>
            </>
          ) : <div className="empty-transcript"><span className="section-kicker">MEMORY VAULT</span><h2>Select a memory to inspect it.</h2><p>Changes are written to Markdown first; the search index is rebuilt from those files.</p></div>}
        </div>
      </div>

      <section className={proposals.length === 0 ? "proposal-queue empty-queue" : "proposal-queue"} aria-label="Memory proposal review">
        <div className="memory-list-header">
          <div className="queue-title"><span className="section-kicker">REVIEW QUEUE</span><span>{proposals.length} proposal{proposals.length === 1 ? "" : "s"}</span></div>
          <label className="memory-check"><input checked={autoCommit} onChange={(event) => toggleAutoCommit(event.target.checked)} type="checkbox" /> Commit accepted proposals automatically</label>
        </div>
        {proposals.length > 0 && <p className="proposal-note">Rejecting a proposal leaves the transcript untouched. Deleting a memory is a separate action and suppresses reconstruction from its source turns.</p>}
        {proposals.map((proposal) => {
          const draft = proposalDrafts[proposal.id] ?? proposal.candidate.body;
          return (
            <article className="proposal-card" key={proposal.id}>
              <div className="proposal-card-header"><div><strong>{proposal.candidate.memory_type}</strong><span> - {proposal.candidate.origin} - {proposal.status}</span></div><span>{Math.round(proposal.candidate.confidence * 100)}% confidence</span></div>
              <textarea aria-label={`Proposal ${proposal.id}`} rows={4} value={draft} onChange={(event) => setProposalDrafts((current) => ({ ...current, [proposal.id]: event.target.value }))} />
              <div className="proposal-evidence"><span className="section-kicker">SUPPORTING EXCERPTS</span>{proposal.candidate.evidence.map((evidence) => <blockquote key={evidence.turn_id}>Turn {evidence.turn_id}: {evidence.quote}</blockquote>)}</div>
              <div className="proposal-actions">
                <button className="outline-button" disabled={busy || !draft.trim()} onClick={() => void editProposal(proposal)} type="button">Save edit</button>
                {proposal.status === "needs_review" && <button className="primary-button" disabled={busy || !draft.trim()} onClick={() => void acceptProposal(proposal)} type="button">{autoCommit ? "Accept and save" : "Accept"}</button>}
                {proposal.status === "accepted" && <button className="primary-button" disabled={busy} onClick={() => void commitProposal(proposal)} type="button">Commit accepted memory</button>}
                {proposal.status === "needs_review" && <button className="text-button danger-button" disabled={busy} onClick={() => void rejectProposal(proposal)} type="button">Reject</button>}
              </div>
            </article>
          );
        })}
        {proposals.length === 0 && <p className="muted compact-empty">Nothing needs review.</p>}
      </section>

      <section className={conflicts.length === 0 ? "proposal-queue empty-queue" : "proposal-queue"} aria-label="Memory conflicts">
        <div className="memory-list-header">
          <div className="queue-title"><span className="section-kicker">CONFLICTS</span><span>{conflicts.length} conflict{conflicts.length === 1 ? "" : "s"}</span></div>
        </div>
        {conflicts.map((pair) => (
          <article className="proposal-card" key={`${pair.relation}:${pair.from.id}:${pair.to.id}:${pair.proposal_id}`}>
            <div className="proposal-card-header"><div><strong>{pair.relation}</strong></div></div>
            <div className="conflict-sides">
              <div className="conflict-side-column">
                <button className="conflict-side" onClick={() => selectConflictSide(pair.from)} type="button">
                  <strong>{pair.from.id}</strong>
                  <span>{pair.from.memory_type}{pair.from.locked ? " · locked" : ""}</span>
                  <p>{pair.from.excerpt}</p>
                </button>
                {pair.relation === "contradicts" && <button className="outline-button" disabled={busy} onClick={() => void excludeConflictSide(pair, pair.from.id)} type="button">Exclude this</button>}
              </div>
              <div className="conflict-side-column">
                <button className="conflict-side" onClick={() => selectConflictSide(pair.to)} type="button">
                  <strong>{pair.to.id}</strong>
                  <span>{pair.to.memory_type}{pair.to.locked ? " · locked" : ""}</span>
                  <p>{pair.to.excerpt}</p>
                </button>
                {pair.relation === "contradicts" && <button className="outline-button" disabled={busy} onClick={() => void excludeConflictSide(pair, pair.to.id)} type="button">Exclude this</button>}
                {pair.relation === "supersedes" && <button className="outline-button" disabled={busy} onClick={() => void excludeConflictSide(pair, pair.to.id)} type="button">Exclude older</button>}
              </div>
            </div>
          </article>
        ))}
        {conflicts.length === 0 && <p className="muted compact-empty">No open conflicts.</p>}
      </section>
      {error && <p className="conversation-error" role="alert">{error}</p>}
    </section>
  );
}
