import { useMemo, useState } from "react";
import {
  AlertCircle,
  Check,
  CheckCheck,
  Download,
  Loader2,
  Music,
  Plus,
  RefreshCw,
  Trash2,
  Users,
  X,
} from "lucide-react";
import { useSubsStore } from "../store/useSubs";
import { useFormatStore } from "../store/useFormat";
import { useLocaleStore, useT } from "../i18n";
import { formatAgo, formatDuration } from "../lib/format";
import type { AutoDownload, Channel, FeedItem, Mode } from "../lib/types";
import { EmptyState } from "./EmptyState";

function ItemRow({
  item,
  checked,
  onToggle,
}: {
  item: FeedItem;
  checked: boolean;
  onToggle: () => void;
}) {
  const t = useT();
  const locale = useLocaleStore((s) => s.locale);
  const { setItemsState } = useSubsStore();
  const handled = item.state !== "new";

  return (
    <div
      className="flex gap-3 rounded-xl border p-2.5"
      style={{ borderColor: "var(--border)", background: "var(--bg-elevated)", opacity: handled ? 0.6 : 1 }}
    >
      <input
        type="checkbox"
        checked={checked}
        onChange={onToggle}
        className="mt-1 h-4 w-4 shrink-0 accent-[var(--accent)]"
      />
      <div className="relative h-[54px] w-24 shrink-0 overflow-hidden rounded-md" style={{ background: "var(--bg-elevated-2)" }}>
        {item.thumbnail && <img src={item.thumbnail} alt="" loading="lazy" className="h-full w-full object-cover" />}
        {item.duration != null && (
          <span
            className="absolute bottom-0.5 right-0.5 rounded px-1 text-[10px] font-medium"
            style={{ background: "rgba(0,0,0,0.75)", color: "white" }}
          >
            {formatDuration(item.duration)}
          </span>
        )}
      </div>
      <div className="flex min-w-0 flex-1 flex-col gap-0.5">
        <button onClick={onToggle} className="line-clamp-2 text-left text-sm font-medium leading-snug">
          {item.title}
        </button>
        <div className="flex items-center gap-1.5 text-xs" style={{ color: "var(--text-muted)" }}>
          <span className="truncate">{item.channelTitle}</span>
          {item.published != null && <span className="shrink-0">· {formatAgo(item.published, locale)}</span>}
          {item.state === "queued" && (
            <span className="shrink-0" style={{ color: "var(--success)" }}>
              · {t("subs.queued")}
            </span>
          )}
        </div>
      </div>
      {item.state === "new" && (
        <button
          onClick={() => void setItemsState([item.videoId], "seen")}
          className="h-fit shrink-0 rounded-md p-1 opacity-50 hover:opacity-100"
          title={t("subs.skip")}
        >
          <X size={14} />
        </button>
      )}
    </div>
  );
}

function Inbox() {
  const t = useT();
  const { items, channels, setItemsState, download } = useSubsStore();
  const spec = useFormatStore((s) => s.spec);
  const [showHandled, setShowHandled] = useState(false);
  const [selected, setSelected] = useState<Set<string>>(new Set());

  const visible = useMemo(() => {
    const enabled = new Set(channels.filter((c) => c.enabled).map((c) => c.id));
    return items
      .filter((i) => enabled.has(i.channelId) && (showHandled || i.state === "new"))
      .sort((a, b) => (b.published ?? b.firstSeen) - (a.published ?? a.firstSeen));
  }, [items, channels, showHandled]);

  const newIds = visible.filter((i) => i.state === "new").map((i) => i.videoId);
  const selectedItems = visible.filter((i) => selected.has(i.videoId));

  const toggle = (id: string) =>
    setSelected((prev) => {
      const next = new Set(prev);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });

  const downloadSelected = async (mode: Mode) => {
    await download(selectedItems, { ...spec, mode });
    setSelected(new Set());
  };

  return (
    <div className="flex flex-col gap-2.5">
      <div className="flex flex-wrap items-center gap-2 text-xs">
        <label className="flex cursor-pointer items-center gap-1.5" style={{ color: "var(--text-muted)" }}>
          <input
            type="checkbox"
            checked={showHandled}
            onChange={(e) => setShowHandled(e.target.checked)}
            className="h-3.5 w-3.5 accent-[var(--accent)]"
          />
          {t("subs.showHandled")}
        </label>
        <div className="ml-auto flex gap-1.5">
          {visible.length > 0 && (
            <button
              onClick={() =>
                setSelected(selected.size === visible.length ? new Set() : new Set(visible.map((i) => i.videoId)))
              }
              className="rounded-md border px-2.5 py-1"
              style={{ borderColor: "var(--border)" }}
            >
              {selected.size === visible.length ? t("playlist.deselectAll") : t("playlist.selectAll")}
            </button>
          )}
          {newIds.length > 0 && (
            <button
              onClick={() => void setItemsState(newIds, "seen")}
              className="flex items-center gap-1 rounded-md border px-2.5 py-1"
              style={{ borderColor: "var(--border)" }}
            >
              <CheckCheck size={12} />
              {t("subs.markAllSeen")}
            </button>
          )}
        </div>
      </div>

      {selectedItems.length > 0 && (
        <div
          className="sticky top-0 z-10 flex flex-wrap items-center gap-2 rounded-xl border p-2.5"
          style={{ borderColor: "var(--accent)", background: "var(--bg-elevated)" }}
        >
          <span className="text-xs font-medium">{t("subs.selected", { count: selectedItems.length })}</span>
          <div className="ml-auto flex gap-1.5">
            <button
              onClick={() => void downloadSelected("video")}
              className="flex items-center gap-1 rounded-md px-2.5 py-1 text-xs font-medium"
              style={{ background: "var(--accent)", color: "white" }}
            >
              <Download size={12} />
              {t("format.video")}
            </button>
            <button
              onClick={() => void downloadSelected("audio")}
              className="flex items-center gap-1 rounded-md px-2.5 py-1 text-xs font-medium"
              style={{ background: "var(--accent)", color: "white" }}
            >
              <Music size={12} />
              {t("format.audio")}
            </button>
            <button
              onClick={() => {
                void setItemsState(
                  selectedItems.map((i) => i.videoId),
                  "seen",
                );
                setSelected(new Set());
              }}
              className="flex items-center gap-1 rounded-md border px-2.5 py-1 text-xs"
              style={{ borderColor: "var(--border)" }}
            >
              <Check size={12} />
              {t("subs.skip")}
            </button>
          </div>
        </div>
      )}

      {visible.length === 0 ? (
        <EmptyState label={channels.length === 0 ? t("subs.noChannels") : t("subs.inboxEmpty")} />
      ) : (
        visible.map((item) => (
          <ItemRow
            key={item.videoId}
            item={item}
            checked={selected.has(item.videoId)}
            onToggle={() => toggle(item.videoId)}
          />
        ))
      )}
    </div>
  );
}

function ChannelRow({ channel }: { channel: Channel }) {
  const t = useT();
  const { updateChannel, removeChannel } = useSubsStore();

  return (
    <div
      className="flex items-center gap-2.5 rounded-xl border p-2.5"
      style={{ borderColor: "var(--border)", background: "var(--bg-elevated)", opacity: channel.enabled ? 1 : 0.55 }}
    >
      <input
        type="checkbox"
        checked={channel.enabled}
        onChange={(e) => void updateChannel(channel.id, { enabled: e.target.checked })}
        title={t("subs.channelEnabled")}
        className="h-4 w-4 shrink-0 accent-[var(--accent)]"
      />
      {channel.thumbnail ? (
        <img src={channel.thumbnail} alt="" loading="lazy" className="h-8 w-8 shrink-0 rounded-full object-cover" />
      ) : (
        <div className="h-8 w-8 shrink-0 rounded-full" style={{ background: "var(--bg-elevated-2)" }} />
      )}
      <div className="flex min-w-0 flex-1 flex-col">
        <span className="truncate text-sm font-medium">{channel.title}</span>
        {channel.lastError && (
          <span className="flex items-center gap-1 truncate text-xs" style={{ color: "var(--danger)" }} title={channel.lastError}>
            <AlertCircle size={11} className="shrink-0" />
            {channel.lastError}
          </span>
        )}
      </div>
      <select
        value={channel.autoDownload ?? ""}
        onChange={(e) => void updateChannel(channel.id, { autoDownload: (e.target.value || null) as AutoDownload | null })}
        title={t("subs.autoDownloadHint")}
        className="shrink-0 rounded-md border bg-transparent px-1.5 py-1 text-xs outline-none"
        style={{ borderColor: "var(--border)", color: "var(--text)", background: "var(--bg)" }}
      >
        <option value="">{t("subs.autoOff")}</option>
        <option value="video">{t("subs.autoVideo")}</option>
        <option value="audio">{t("subs.autoAudio")}</option>
      </select>
      <button
        onClick={() => {
          if (confirm(t("subs.removeConfirm", { title: channel.title }))) void removeChannel(channel.id);
        }}
        className="shrink-0 rounded-md p-1 opacity-50 hover:opacity-100"
        title={t("queue.remove")}
      >
        <Trash2 size={14} />
      </button>
    </div>
  );
}

function Channels() {
  const t = useT();
  const { channels, addChannel, importChannels } = useSubsStore();
  const [url, setUrl] = useState("");
  const [busy, setBusy] = useState<"add" | "import" | null>(null);
  const [message, setMessage] = useState<{ text: string; error: boolean } | null>(null);

  const run = async (kind: "add" | "import", action: () => Promise<string>) => {
    setBusy(kind);
    setMessage(null);
    try {
      setMessage({ text: await action(), error: false });
    } catch (e) {
      setMessage({ text: String(e), error: true });
    } finally {
      setBusy(null);
    }
  };

  const sorted = [...channels].sort((a, b) => a.title.localeCompare(b.title));

  return (
    <div className="flex flex-col gap-2.5">
      <form
        onSubmit={(e) => {
          e.preventDefault();
          if (!url.trim()) return;
          void run("add", async () => {
            const channel = await addChannel(url);
            setUrl("");
            return t("subs.added", { title: channel.title });
          });
        }}
        className="flex gap-2"
      >
        <input
          value={url}
          onChange={(e) => setUrl(e.target.value)}
          placeholder={t("subs.addPlaceholder")}
          className="min-w-0 flex-1 rounded-lg border px-3 py-1.5 text-sm outline-none"
          style={{ borderColor: "var(--border)", background: "var(--bg)", color: "var(--text)" }}
        />
        <button
          type="submit"
          disabled={busy !== null || !url.trim()}
          className="flex shrink-0 items-center gap-1 rounded-lg px-3 py-1.5 text-sm font-medium disabled:opacity-50"
          style={{ background: "var(--accent)", color: "white" }}
        >
          {busy === "add" ? <Loader2 size={14} className="animate-spin" /> : <Plus size={14} />}
          {t("subs.add")}
        </button>
      </form>

      <button
        onClick={() =>
          void run("import", async () => t("subs.imported", { count: await importChannels() }))
        }
        disabled={busy !== null}
        className="flex w-fit items-center gap-1.5 rounded-lg border px-3 py-1.5 text-xs disabled:opacity-50"
        style={{ borderColor: "var(--border)" }}
        title={t("subs.importHint")}
      >
        {busy === "import" ? <Loader2 size={12} className="animate-spin" /> : <Users size={12} />}
        {t("subs.import")}
      </button>

      {message && (
        <p className="text-xs" style={{ color: message.error ? "var(--danger)" : "var(--text-muted)" }}>
          {message.text}
        </p>
      )}

      {sorted.length === 0 ? (
        <EmptyState label={t("subs.noChannels")} />
      ) : (
        sorted.map((c) => <ChannelRow key={c.id} channel={c} />)
      )}
    </div>
  );
}

export function SubscriptionsPanel() {
  const t = useT();
  const locale = useLocaleStore((s) => s.locale);
  const { refreshing, progress, lastRefresh, channels, refresh } = useSubsStore();
  const [view, setView] = useState<"inbox" | "channels">(channels.length === 0 ? "channels" : "inbox");

  return (
    <div className="flex flex-col gap-3 pb-8">
      <div className="flex items-center gap-2">
        <div className="flex rounded-lg border p-0.5 text-xs" style={{ borderColor: "var(--border)" }}>
          {(["inbox", "channels"] as const).map((v) => (
            <button
              key={v}
              onClick={() => setView(v)}
              className="rounded-md px-2.5 py-1 font-medium"
              style={{
                background: view === v ? "var(--accent-muted)" : "transparent",
                color: view === v ? "var(--accent)" : "var(--text-muted)",
              }}
            >
              {v === "inbox" ? t("subs.inbox") : t("subs.channels", { count: channels.length })}
            </button>
          ))}
        </div>
        <span className="ml-auto truncate text-xs" style={{ color: "var(--text-faint)" }}>
          {refreshing && progress
            ? t("subs.progress", { done: progress.done, total: progress.total })
            : lastRefresh
              ? t("subs.lastRefresh", { ago: formatAgo(lastRefresh, locale) })
              : null}
        </span>
        <button
          onClick={() => void refresh()}
          disabled={refreshing || channels.length === 0}
          className="flex shrink-0 items-center gap-1 rounded-lg border px-2.5 py-1 text-xs disabled:opacity-50"
          style={{ borderColor: "var(--border)" }}
        >
          <RefreshCw size={12} className={refreshing ? "animate-spin" : undefined} />
          {t("subs.refresh")}
        </button>
      </div>

      {view === "inbox" ? <Inbox /> : <Channels />}

      <p className="text-[11px]" style={{ color: "var(--text-faint)" }}>
        {t("subs.footer")}
      </p>
    </div>
  );
}
