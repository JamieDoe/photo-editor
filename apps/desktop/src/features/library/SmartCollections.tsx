import { useEffect, useState, type FormEvent } from "react";
import { PlusIcon, SmartIcon, TrashIcon } from "../../components/icons";
import { Popover, PopoverHeader, PopoverIcon } from "../../components/Popover";
import * as ipc from "../../ipc/client";
import type { ColourLabelDto } from "../../ipc/generated/ColourLabelDto";
import type { FlagRuleDto } from "../../ipc/generated/FlagRuleDto";
import type { SmartCollectionDto } from "../../ipc/generated/SmartCollectionDto";
import type { SmartRulesDto } from "../../ipc/generated/SmartRulesDto";
import { photosText } from "./Albums";
import { LABELS } from "./marks";
import { NavGroup } from "./NavGroup";
import { EDITED_RULES, FLAG_RULES, NO_RULES, RATING_RULES, hasNoRules, rulesSummary } from "./smart";
import type { LibraryApi } from "./useLibrary";

/** Smart collections (ADR 0084) in the sidebar, after Albums: each with its count; +
 *  opens the rules for a new one. */
export function SmartCollectionsNav({ library, onNew }: { library: LibraryApi; onNew: (anchor: HTMLElement) => void }) {
  const all = library.smartCollections;
  return (
    <NavGroup
      label="Smart collections"
      action={
        <button className="icon-button" aria-label="New smart collection" title="New smart collection" onClick={(e) => onNew(e.currentTarget)}>
          <PlusIcon size={13} />
        </button>
      }
    >
      {all.length === 0 && (
        <div className="nav-empty-card">
          <SmartIcon size={18} />
          <p>Collections that fill themselves: every photo matching the rules you choose.</p>
          <button className="ghost small" onClick={(e) => onNew(e.currentTarget)}>
            <PlusIcon size={12} />
            New smart collection
          </button>
        </div>
      )}
      {all.map((c) => (
        <button
          key={c.id}
          className="nav-row"
          aria-current={!library.search && library.smart?.collection.id === c.id ? "true" : undefined}
          title={`${c.name}: ${rulesSummary(c.rules)}`}
          onClick={() => void library.openSmart(c.id)}
        >
          <SmartIcon size={16} />
          <span className="grow">{c.name}</span>
          <span className="count">{c.count.toLocaleString()}</span>
        </button>
      ))}
      {all.length > 0 && (
        <button className="nav-row nav-add" onClick={(e) => onNew(e.currentTarget)}>
          <PlusIcon size={14} />
          <span className="grow">New smart collection</span>
        </button>
      )}
    </NavGroup>
  );
}

/** How long the rules rest before the matching photos are counted. */
const COUNT_DELAY_MS = 150;

/**
 * A smart collection's name and rules: a new one (`editing` null; `start` fills it in,
 * say from a search), or one to change or delete. Shows how many photos match as the
 * rules change.
 */
export function SmartCollectionEditor({
  library,
  editing,
  start,
  anchor,
  onClose,
}: {
  library: LibraryApi;
  editing: SmartCollectionDto | null;
  start?: { name: string; rules: SmartRulesDto };
  anchor: HTMLElement;
  onClose: () => void;
}) {
  const [name, setName] = useState(editing?.name ?? start?.name ?? "");
  const [rules, setRules] = useState<SmartRulesDto>(editing?.rules ?? start?.rules ?? NO_RULES);
  const [matching, setMatching] = useState<number | null>(editing?.count ?? null);
  const [confirming, setConfirming] = useState(false);
  const noRules = hasNoRules(rules);

  useEffect(() => {
    if (noRules) {
      setMatching(null);
      return;
    }
    let current = true;
    const t = window.setTimeout(() => {
      ipc.countSmartRules(rules).then(
        (n) => current && setMatching(n),
        () => current && setMatching(null),
      );
    }, COUNT_DELAY_MS);
    return () => {
      current = false;
      window.clearTimeout(t);
    };
  }, [rules, noRules]);

  const change = (patch: Partial<SmartRulesDto>) => setRules((r) => ({ ...r, ...patch }));
  const save = async (e: FormEvent) => {
    e.preventDefault();
    if (!name.trim() || noRules) return;
    const done = editing ? await library.updateSmart(editing.id, name, rules) : await library.createSmart(name, rules);
    if (done) onClose();
  };
  const sub = noRules ? "Choose at least one rule" : matching === null ? "Counting…" : `${photosText(matching)} match`;
  return (
    <Popover anchor={anchor} label={editing ? `Smart collection ${editing.name}` : "New smart collection"} onClose={onClose}>
      <PopoverHeader
        visual={
          <PopoverIcon>
            <SmartIcon size={18} />
          </PopoverIcon>
        }
        title={<span className="popover-title">{editing ? editing.name : "New smart collection"}</span>}
        sub={sub}
      />
      <form className="popover-body smart-rules" onSubmit={(e) => void save(e)}>
        <input
          className="text-input"
          autoFocus={!editing}
          maxLength={60}
          placeholder="Name"
          aria-label="Smart collection name"
          value={name}
          onChange={(e) => setName(e.target.value)}
        />
        <p className="popover-text small">Photos that match every rule:</p>
        <RuleSelect
          label="Rating"
          options={RATING_RULES}
          value={rules.rating}
          onChange={(rating) => change({ rating })}
        />
        <RuleSelect<FlagRuleDto | null> label="Flag" options={FLAG_RULES} value={rules.flag} onChange={(flag) => change({ flag })} />
        <RuleSelect<ColourLabelDto | null>
          label="Label"
          options={[
            { value: null, label: "Any label" },
            { value: "none", label: "No label" },
            ...LABELS.map((l) => ({ value: l.id, label: l.name })),
          ]}
          value={rules.label}
          onChange={(label) => change({ label })}
        />
        <RuleSelect label="Edited" options={EDITED_RULES} value={rules.edited} onChange={(edited) => change({ edited })} />
        <label className="rule-row">
          <span>Words</span>
          <input
            className="text-input"
            maxLength={200}
            placeholder="Folders, cameras, dates"
            value={rules.words}
            onChange={(e) => change({ words: e.target.value })}
          />
        </label>
        <div className="popover-actions">
          {confirming && editing ? (
            <>
              <span className="popover-text small grow">Delete it? Its photos stay.</span>
              <button className="ghost small" type="button" onClick={() => setConfirming(false)}>
                Keep
              </button>
              <button
                className="danger small"
                type="button"
                onClick={() => {
                  void library.deleteSmart(editing.id);
                  onClose();
                }}
              >
                Delete
              </button>
            </>
          ) : (
            <>
              {editing && (
                <button className="ghost small album-delete" type="button" onClick={() => setConfirming(true)}>
                  <TrashIcon size={13} />
                  Delete
                </button>
              )}
              <button className="primary small" type="submit" disabled={!name.trim() || noRules}>
                {editing ? "Save" : "Create"}
              </button>
            </>
          )}
        </div>
      </form>
    </Popover>
  );
}

/** One rule's choices, as a labelled select. Values are matched by their position, so
 *  null, numbers and booleans all work. */
function RuleSelect<T>({
  label,
  options,
  value,
  onChange,
}: {
  label: string;
  options: ReadonlyArray<{ value: T; label: string }>;
  value: T;
  onChange: (v: T) => void;
}) {
  const index = Math.max(
    0,
    options.findIndex((o) => o.value === value),
  );
  return (
    <label className="rule-row">
      <span>{label}</span>
      <select className="rule-select" value={index} onChange={(e) => onChange(options[Number(e.target.value)]!.value)}>
        {options.map((o, i) => (
          <option key={o.label} value={i}>
            {o.label}
          </option>
        ))}
      </select>
    </label>
  );
}
