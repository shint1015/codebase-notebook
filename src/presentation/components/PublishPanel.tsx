import { useState } from "react";
import type { Repository } from "../../domain/types";
import { api } from "../../infrastructure/api";
import { isCommandError } from "../../domain/types";

interface Props {
  repositories: Repository[];
  /** Called after a wiki page lands so the workspace can be re-indexed. */
  onWikiPublished: () => void;
}

const isWiki = (repo: Repository) =>
  repo.source_kind === "git" && (repo.remote_url ?? "").includes(".wiki");

/** Outbound side: create GitHub issues and wiki pages (explicit action only). */
export function PublishPanel({ repositories, onWikiPublished }: Props) {
  const wikis = repositories.filter(isWiki);

  // --- issue form ---
  const [issueSpec, setIssueSpec] = useState("");
  const [issueTitle, setIssueTitle] = useState("");
  const [issueBody, setIssueBody] = useState("");
  const [issueStatus, setIssueStatus] = useState<string | null>(null);
  const [issueUrl, setIssueUrl] = useState<string | null>(null);
  const [issueBusy, setIssueBusy] = useState(false);

  // --- wiki form ---
  const [wikiRepoId, setWikiRepoId] = useState("");
  const [wikiTitle, setWikiTitle] = useState("");
  const [wikiContent, setWikiContent] = useState("");
  const [wikiStatus, setWikiStatus] = useState<string | null>(null);
  const [wikiBusy, setWikiBusy] = useState(false);

  const createIssue = async () => {
    setIssueBusy(true);
    setIssueStatus(null);
    setIssueUrl(null);
    try {
      const url = await api.createGithubIssue(issueSpec, issueTitle, issueBody);
      setIssueUrl(url);
      setIssueStatus("Created:");
      setIssueTitle("");
      setIssueBody("");
    } catch (e) {
      setIssueStatus(isCommandError(e) ? e.message : String(e));
    } finally {
      setIssueBusy(false);
    }
  };

  const publishWiki = async () => {
    const repoId = wikiRepoId || wikis[0]?.id;
    if (!repoId) return;
    setWikiBusy(true);
    setWikiStatus(null);
    try {
      const file = await api.writeWikiPage(repoId, wikiTitle, wikiContent);
      setWikiStatus(`Published ${file} — re-indexing…`);
      setWikiTitle("");
      setWikiContent("");
      onWikiPublished();
    } catch (e) {
      setWikiStatus(isCommandError(e) ? e.message : String(e));
    } finally {
      setWikiBusy(false);
    }
  };

  return (
    <section className="home-section">
      <div className="home-section-header">
        <h3>Publish</h3>
      </div>
      <div className="publish-grid">
        <div className="publish-card">
          <h4>New GitHub issue</h4>
          <input
            value={issueSpec}
            placeholder="owner/repo"
            onChange={(e) => setIssueSpec(e.target.value)}
          />
          <input
            value={issueTitle}
            placeholder="Issue title"
            onChange={(e) => setIssueTitle(e.target.value)}
          />
          <textarea
            value={issueBody}
            placeholder="Issue body (markdown)"
            rows={5}
            onChange={(e) => setIssueBody(e.target.value)}
          />
          <div className="publish-actions">
            <button
              className="primary"
              disabled={issueBusy || !issueSpec.trim() || !issueTitle.trim()}
              onClick={() => void createIssue()}
            >
              {issueBusy ? "Creating…" : "Create issue"}
            </button>
            {issueStatus && (
              <span className="provider-status">
                {issueStatus}{" "}
                {issueUrl && (
                  <a href={issueUrl} target="_blank" rel="noreferrer">
                    {issueUrl}
                  </a>
                )}
              </span>
            )}
          </div>
        </div>

        <div className="publish-card">
          <h4>New wiki page</h4>
          {wikis.length === 0 ? (
            <p className="settings-note">
              Clone a wiki first (git URL ending in <code>.wiki.git</code>) to
              publish pages.
            </p>
          ) : (
            <>
              <select
                value={wikiRepoId || wikis[0].id}
                onChange={(e) => setWikiRepoId(e.target.value)}
              >
                {wikis.map((repo) => (
                  <option key={repo.id} value={repo.id}>
                    {repo.name}
                  </option>
                ))}
              </select>
              <input
                value={wikiTitle}
                placeholder="Page title (e.g. Deployment Guide)"
                onChange={(e) => setWikiTitle(e.target.value)}
              />
              <textarea
                value={wikiContent}
                placeholder="Page content (markdown)"
                rows={5}
                onChange={(e) => setWikiContent(e.target.value)}
              />
              <div className="publish-actions">
                <button
                  className="primary"
                  disabled={wikiBusy || !wikiTitle.trim() || !wikiContent.trim()}
                  onClick={() => void publishWiki()}
                >
                  {wikiBusy ? "Publishing…" : "Publish page"}
                </button>
                {wikiStatus && <span className="provider-status">{wikiStatus}</span>}
              </div>
            </>
          )}
        </div>
      </div>
    </section>
  );
}
