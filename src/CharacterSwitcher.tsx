import { useEffect, useMemo, useState } from "react";
import { CharacterPortraitMark } from "./CharacterPortrait";
import { characterClient, type CharacterLibraryItem } from "./characters";
import { filterCharacterChoices } from "./chatShortcuts";

function sameVault(left: string, right: string) {
  return left.replace(/[\\/]+$/, "").toLowerCase() === right.replace(/[\\/]+$/, "").toLowerCase();
}

export function CharacterSwitcher({
  activeVaultRoot,
  onClose,
  onSelect,
}: {
  activeVaultRoot: string;
  onClose: () => void;
  onSelect: (vaultRoot: string) => void;
}) {
  const [query, setQuery] = useState("");
  const [entries, setEntries] = useState<CharacterLibraryItem[]>([]);
  const [portraits, setPortraits] = useState<Record<string, string>>({});
  const [error, setError] = useState<string | null>(null);
  const [activeIndex, setActiveIndex] = useState(0);

  useEffect(() => {
    let cancelled = false;
    void (async () => {
      try {
        const view = await characterClient.listLibrary();
        if (cancelled) return;
        setEntries(view.entries);
        const current = filterCharacterChoices(view.entries, "").findIndex((entry) => (
          sameVault(entry.vault_root, activeVaultRoot)
        ));
        setActiveIndex(current >= 0 ? current : 0);
        const next: Record<string, string> = {};
        await Promise.all(
          view.entries
            .filter((entry) => entry.has_portrait && !entry.error)
            .map(async (entry) => {
              const src = await characterClient.loadPortrait(entry.vault_root);
              if (src) next[entry.vault_root] = src;
            }),
        );
        if (!cancelled) setPortraits(next);
      } catch (requestError) {
        if (!cancelled) {
          setError(requestError instanceof Error ? requestError.message : String(requestError));
        }
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [activeVaultRoot]);

  const choices = useMemo(() => filterCharacterChoices(entries, query), [entries, query]);
  const highlightIndex = choices.length === 0 ? 0 : Math.min(activeIndex, choices.length - 1);

  function highlightCurrentOrFirst(listed: CharacterLibraryItem[], nextQuery: string) {
    const nextChoices = filterCharacterChoices(listed, nextQuery);
    if (nextQuery.trim()) {
      setActiveIndex(0);
      return;
    }
    const selected = nextChoices.findIndex((entry) => sameVault(entry.vault_root, activeVaultRoot));
    setActiveIndex(selected >= 0 ? selected : 0);
  }

  function choose(index: number) {
    const entry = choices[index];
    if (!entry) return;
    onSelect(entry.vault_root);
  }

  function onQueryChange(value: string) {
    setQuery(value);
    highlightCurrentOrFirst(entries, value);
  }

  return (
    <div
      className="character-switcher-backdrop"
      onMouseDown={(event) => {
        if (event.target === event.currentTarget) onClose();
      }}
    >
      <div
        className="character-switcher"
        role="dialog"
        aria-modal="true"
        aria-label="Switch character"
        onKeyDown={(event) => {
          if (event.key === "ArrowDown") {
            event.preventDefault();
            setActiveIndex((current) => (choices.length === 0 ? 0 : (current + 1) % choices.length));
          } else if (event.key === "ArrowUp") {
            event.preventDefault();
            setActiveIndex((current) => (
              choices.length === 0 ? 0 : (current - 1 + choices.length) % choices.length
            ));
          } else if (event.key === "Enter") {
            event.preventDefault();
            choose(highlightIndex);
          }
        }}
      >
        <p className="eyebrow">Switch character</p>
        <label>
          <span className="visually-hidden">Filter characters</span>
          <input
            autoFocus
            onChange={(event) => onQueryChange(event.target.value)}
            placeholder="Type a name"
            value={query}
          />
        </label>
        {error && <p className="inline-status" role="alert">{error}</p>}
        {choices.length === 0 ? (
          <p className="muted memory-empty">
            {!query.trim()
              ? "No characters in the library yet."
              : "No characters match that search."}
          </p>
        ) : (
          <ul className="character-switcher-list" role="listbox" aria-label="Characters">
            {choices.map((entry, index) => {
              const selected = sameVault(entry.vault_root, activeVaultRoot);
              const active = index === highlightIndex;
              const name = entry.name || entry.character_id || "Unnamed";
              return (
                <li key={entry.vault_root}>
                  <button
                    aria-selected={active}
                    className={[
                      "memory-row",
                      "character-library-row",
                      active ? "selected" : "",
                    ].filter(Boolean).join(" ")}
                    id={`character-choice-${index}`}
                    onClick={() => onSelect(entry.vault_root)}
                    onMouseEnter={() => setActiveIndex(index)}
                    role="option"
                    type="button"
                  >
                    <CharacterPortraitMark
                      className="avatar avatar-sm"
                      name={name}
                      src={portraits[entry.vault_root] ?? null}
                    />
                    <span className="character-library-copy">
                      <strong>{name}</strong>
                      <span>{selected ? "Current · " : ""}{entry.character_id}</span>
                    </span>
                  </button>
                </li>
              );
            })}
          </ul>
        )}
        <p className="muted character-switcher-hint">Enter to open · Esc to close</p>
        <button className="text-button" onClick={onClose} type="button">Cancel</button>
      </div>
    </div>
  );
}
