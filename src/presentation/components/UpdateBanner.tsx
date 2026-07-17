import { useEffect, useState } from "react";
import { check, type Update } from "@tauri-apps/plugin-updater";
import { relaunch } from "@tauri-apps/plugin-process";

/**
 * Checks for a new release on startup and offers a one-click update.
 * The check hits GitHub Releases; updates are signature-verified by Tauri
 * before anything is installed. Nothing happens without the user clicking.
 */
export function UpdateBanner() {
  const [update, setUpdate] = useState<Update | null>(null);
  const [progress, setProgress] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [dismissed, setDismissed] = useState(false);

  useEffect(() => {
    check()
      .then((found) => {
        if (found?.available) setUpdate(found);
      })
      // Offline or no release yet — updates are optional, never block the app.
      .catch(() => {});
  }, []);

  if (!update || dismissed) return null;

  const install = async () => {
    setError(null);
    let downloaded = 0;
    try {
      await update.downloadAndInstall((event) => {
        switch (event.event) {
          case "Started":
            setProgress("Downloading…");
            break;
          case "Progress":
            downloaded += event.data.chunkLength;
            setProgress(`Downloading… ${(downloaded / 1_000_000).toFixed(1)} MB`);
            break;
          case "Finished":
            setProgress("Installing…");
            break;
        }
      });
      setProgress("Restarting…");
      await relaunch();
    } catch (e) {
      setError(String(e));
      setProgress(null);
    }
  };

  return (
    <div className="update-banner">
      <span>
        <strong>Update available — v{update.version}</strong>
        {update.body ? ` · ${update.body.split("\n")[0]}` : ""}
      </span>
      <span className="update-actions">
        {progress ? (
          <span className="update-progress">{progress}</span>
        ) : (
          <>
            <button className="primary" onClick={() => void install()}>
              Update &amp; restart
            </button>
            <button onClick={() => setDismissed(true)}>Later</button>
          </>
        )}
      </span>
      {error && <div className="error">{error}</div>}
    </div>
  );
}
