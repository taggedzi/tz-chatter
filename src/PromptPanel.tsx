import { FormEvent, useEffect, useState } from "react";
import { characterClient, DEFAULT_APPLICATION_PROMPT, type ApplicationPrompt } from "./characters";

export function PromptPanel() {
  const [text, setText] = useState(DEFAULT_APPLICATION_PROMPT);
  const [status, setStatus] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    let cancelled = false;
    void characterClient
      .loadPrompt()
      .then((prompt) => {
        if (!cancelled) setText(prompt.text);
      })
      .catch((error: unknown) => {
        if (!cancelled) setStatus(error instanceof Error ? error.message : String(error));
      });
    return () => {
      cancelled = true;
    };
  }, []);

  async function save(event: FormEvent) {
    event.preventDefault();
    setBusy(true);
    setStatus(null);
    try {
      const prompt: ApplicationPrompt = { schema_version: 1, text };
      await characterClient.savePrompt(prompt);
      setStatus(
        text.trim()
          ? "Application prompt saved. It is sent before character identity and memories."
          : "Application prompt cleared. Character identity will be the first system message.",
      );
    } catch (error) {
      setStatus(error instanceof Error ? error.message : String(error));
    } finally {
      setBusy(false);
    }
  }

  return (
    <section className="settings-section" aria-labelledby="prompt-heading">
      <div className="section-heading">
        <div>
          <span className="section-kicker">APPLICATION RULES</span>
          <h2 id="prompt-heading">Set the prompt that comes before every character.</h2>
        </div>
      </div>
      <p className="panel-description">
        This text is stored with the app, not in a character vault. It is the first system message on every turn. Leave it blank to skip it.
      </p>
      <form className="prompt-form" onSubmit={(event) => void save(event)}>
        <label>
          Application prompt
          <textarea
            onChange={(event) => setText(event.target.value)}
            rows={10}
            value={text}
          />
        </label>
        <div className="action-row">
          <button className="primary-button" disabled={busy} type="submit">
            {busy ? "Saving…" : "Save prompt"}
          </button>
          <button
            className="text-button"
            onClick={() => setText(DEFAULT_APPLICATION_PROMPT)}
            type="button"
          >
            Restore default
          </button>
        </div>
      </form>
      {status && <p className="inline-status">{status}</p>}
    </section>
  );
}
