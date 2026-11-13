import { useState } from "react";
import type { BlobData, FileBlobs } from "../types";

const IMAGE_TYPES: Record<string, string> = {
  png: "image/png",
  jpg: "image/jpeg",
  jpeg: "image/jpeg",
  gif: "image/gif",
  webp: "image/webp",
  avif: "image/avif",
  bmp: "image/bmp",
  ico: "image/x-icon",
  svg: "image/svg+xml",
};

/** The image MIME type for a path, or null if it isn't a previewable image. */
export function imageType(path: string): string | null {
  const ext = path.split(".").pop()?.toLowerCase() ?? "";
  return IMAGE_TYPES[ext] ?? null;
}

export function formatBytes(n: number) {
  if (n < 1024) return `${n} B`;
  if (n < 1024 * 1024) return `${(n / 1024).toFixed(1)} KB`;
  return `${(n / 1024 / 1024).toFixed(1)} MB`;
}

function Side({ label, blob, mime }: { label: string; blob: BlobData | null; mime: string }) {
  // Remember which contents a size or error belongs to, so a new image
  // never shows the previous one's details.
  const [loaded, setLoaded] = useState<{ data: string; w: number; h: number } | null>(null);
  const [failed, setFailed] = useState<string | null>(null);
  const data = blob?.base64 ?? null;
  const size = loaded && loaded.data === data ? loaded : null;
  const broken = data !== null && failed === data;
  return (
    <figure className="image-side">
      <figcaption>
        <span className="image-label">{label}</span>
        {blob && (
          <span className="muted">
            {size ? `${size.w} × ${size.h} · ` : ""}
            {formatBytes(blob.size)}
          </span>
        )}
      </figcaption>
      <div className="image-frame">
        {!blob ? (
          <span className="muted">{label === "Before" ? "Added in this commit" : "Deleted in this commit"}</span>
        ) : !blob.base64 ? (
          <span className="muted">Too large to preview</span>
        ) : broken ? (
          <span className="muted">Can't display this image</span>
        ) : (
          <img
            src={`data:${mime};base64,${blob.base64}`}
            alt={label}
            onLoad={(e) => data && setLoaded({ data, w: e.currentTarget.naturalWidth, h: e.currentTarget.naturalHeight })}
            onError={() => setFailed(data)}
          />
        )}
      </div>
    </figure>
  );
}

export function ImagePreview({ blobs, path }: { blobs: FileBlobs; path: string }) {
  const mime = imageType(path) ?? "application/octet-stream";
  return (
    <div className="image-preview">
      <Side key={`old-${path}`} label="Before" blob={blobs.old} mime={mime} />
      <Side key={`new-${path}`} label="After" blob={blobs.new} mime={mime} />
    </div>
  );
}
