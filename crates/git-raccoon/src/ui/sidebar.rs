//! The left sidebar: local branches, remotes, tags and stashes.

use std::collections::HashSet;

use eframe::egui::{self, Align2, Pos2, Rect, RichText, Sense, Stroke};
use egui_phosphor::regular as icon;
use git_raccoon_core::refs::{LocalBranch, Refs, RemoteBranch};

use crate::theme;
use crate::ui::widgets;

const ROW: f32 = 24.0;
const INDENT: f32 = 14.0;

/// Branch names grouped into folders by "/".
enum Node<'a, T> {
    Folder {
        name: String,
        path: String,
        children: Vec<Node<'a, T>>,
    },
    Leaf {
        name: &'a str,
        item: &'a T,
    },
}

fn build_tree<'a, T>(
    items: impl IntoIterator<Item = &'a T>,
    name_of: impl Fn(&'a T) -> &'a str,
) -> Vec<Node<'a, T>> {
    let mut root: Vec<Node<'a, T>> = Vec::new();
    for item in items {
        let name = name_of(item);
        let parts: Vec<&str> = name.split('/').collect();
        let mut level = &mut root;
        let mut path = String::new();
        for part in &parts[..parts.len() - 1] {
            if !path.is_empty() {
                path.push('/');
            }
            path.push_str(part);
            let index = level
                .iter()
                .position(|n| matches!(n, Node::Folder { name, .. } if name == part))
                .unwrap_or_else(|| {
                    level.push(Node::Folder {
                        name: (*part).to_string(),
                        path: path.clone(),
                        children: Vec::new(),
                    });
                    level.len() - 1
                });
            level = match &mut level[index] {
                Node::Folder { children, .. } => children,
                Node::Leaf { .. } => unreachable!("index points at a folder"),
            };
        }
        level.push(Node::Leaf {
            name: parts[parts.len() - 1],
            item,
        });
    }
    sort(&mut root);
    root
}

/// Folders first; each group keeps its incoming order.
fn sort<T>(nodes: &mut [Node<'_, T>]) {
    nodes.sort_by_key(|n| matches!(n, Node::Leaf { .. }));
    for n in nodes {
        if let Node::Folder { children, .. } = n {
            sort(children);
        }
    }
}

struct Badge {
    text: String,
    colour: egui::Color32,
}

/// One row. Returns its response.
#[expect(
    clippy::too_many_arguments,
    reason = "a private drawing helper; a struct would only add noise"
)]
fn row(
    ui: &mut egui::Ui,
    depth: usize,
    glyph: &str,
    text: &str,
    badges: &[Badge],
    active: bool,
    current: bool,
    tooltip: &str,
) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(
        egui::vec2(ui.available_width(), ROW),
        Sense::click_and_drag(),
    );
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, text));
    let painter = ui.painter();
    if active {
        painter.rect_filled(rect, 0, theme::bg_selected());
    } else if response.hovered() {
        painter.rect_filled(rect, 0, theme::bg_hover());
    }
    #[expect(clippy::cast_precision_loss, reason = "tree depth is small")]
    let x = rect.left() + 12.0 + depth as f32 * INDENT;
    let colour = if current {
        theme::accent()
    } else {
        theme::text()
    };
    painter.text(
        Pos2::new(x + 7.0, rect.center().y),
        Align2::CENTER_CENTER,
        glyph,
        theme::body(13.0),
        if current {
            theme::accent()
        } else {
            theme::muted()
        },
    );

    // Badges from the right edge.
    let mut right = rect.right() - 10.0;
    for b in badges.iter().rev() {
        let galley = painter.layout_no_wrap(b.text.clone(), theme::body(11.0), b.colour);
        let r = Rect::from_min_max(
            Pos2::new(right - galley.size().x - 10.0, rect.center().y - 8.0),
            Pos2::new(right, rect.center().y + 8.0),
        );
        painter.rect_filled(r, 8, theme::bg());
        painter.galley(
            Pos2::new(r.left() + 5.0, rect.center().y - galley.size().y / 2.0),
            galley,
            b.colour,
        );
        right = r.left() - 4.0;
    }
    let font = if current {
        egui::FontId::new(13.0, egui::FontFamily::Proportional)
    } else {
        theme::body(13.0)
    };
    let label = Rect::from_min_max(
        Pos2::new(x + 20.0, rect.top()),
        Pos2::new(right - 4.0, rect.bottom()),
    );
    widgets::text_in(painter, label, text, font, colour);
    if !tooltip.is_empty() {
        response.clone().on_hover_text(tooltip);
    }
    response
}

fn folder_row(
    ui: &mut egui::Ui,
    depth: usize,
    glyph: &str,
    name: &str,
    open: bool,
    tooltip: &str,
) -> egui::Response {
    let (rect, response) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), ROW), Sense::click());
    response
        .widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::CollapsingHeader, true, name));
    let painter = ui.painter();
    if response.hovered() {
        painter.rect_filled(rect, 0, theme::bg_hover());
    }
    #[expect(clippy::cast_precision_loss, reason = "tree depth is small")]
    let x = rect.left() + 4.0 + depth as f32 * INDENT;
    let caret = if open {
        icon::CARET_DOWN
    } else {
        icon::CARET_RIGHT
    };
    painter.text(
        Pos2::new(x + 5.0, rect.center().y),
        Align2::CENTER_CENTER,
        caret,
        theme::body(10.0),
        theme::muted(),
    );
    painter.text(
        Pos2::new(x + 20.0, rect.center().y),
        Align2::CENTER_CENTER,
        glyph,
        theme::body(13.0),
        theme::muted(),
    );
    widgets::text_in(
        painter,
        Rect::from_min_max(Pos2::new(x + 32.0, rect.top()), rect.max),
        name,
        theme::body(13.0),
        theme::text(),
    );
    if tooltip.is_empty() {
        response
    } else {
        response.on_hover_text(tooltip)
    }
}

/// A section heading. Returns (toggled, add clicked); the add button is
/// shown when `add` names it.
fn section_header(
    ui: &mut egui::Ui,
    title: &str,
    count: usize,
    open: bool,
    add: Option<&str>,
) -> (bool, bool) {
    let (rect, response) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), 28.0), Sense::click());
    let mut added = false;
    if let Some(name) = add {
        let r = Rect::from_center_size(
            Pos2::new(rect.right() - 40.0, rect.center().y + 2.0),
            egui::vec2(20.0, 20.0),
        );
        let button = ui.put(
            r,
            egui::Button::new(RichText::new(icon::PLUS).size(12.0)).frame(false),
        );
        ui.ctx()
            .accesskit_node_builder(button.id, |n| n.set_label(name));
        added = button.on_hover_text(name).clicked();
    }
    let painter = ui.painter();
    let caret = if open {
        icon::CARET_DOWN
    } else {
        icon::CARET_RIGHT
    };
    painter.text(
        Pos2::new(rect.left() + 10.0, rect.center().y + 2.0),
        Align2::CENTER_CENTER,
        caret,
        theme::body(10.0),
        theme::muted(),
    );
    painter.text(
        Pos2::new(rect.left() + 20.0, rect.center().y + 2.0),
        Align2::LEFT_CENTER,
        title.to_uppercase(),
        egui::FontId::proportional(11.0),
        theme::muted(),
    );
    painter.text(
        Pos2::new(rect.right() - 12.0, rect.center().y + 2.0),
        Align2::RIGHT_CENTER,
        count.to_string(),
        egui::FontId::proportional(11.0),
        theme::muted(),
    );
    (response.clicked(), added)
}

struct Ctx<'s> {
    collapsed: &'s mut HashSet<String>,
    filtering: bool,
    selected: Option<&'s str>,
    action: Option<Action>,
}

impl Ctx<'_> {
    fn is_open(&self, key: &str) -> bool {
        self.filtering || !self.collapsed.contains(key)
    }

    fn toggle(&mut self, key: &str) {
        if !self.collapsed.remove(key) {
            self.collapsed.insert(key.to_string());
        }
    }
}

fn tree<T>(
    ui: &mut egui::Ui,
    c: &mut Ctx<'_>,
    nodes: &[Node<'_, T>],
    prefix: &str,
    depth: usize,
    leaf: &mut dyn FnMut(&mut egui::Ui, &mut Ctx<'_>, &T, &str, usize),
) {
    for node in nodes {
        match node {
            Node::Leaf { name, item } => leaf(ui, c, item, name, depth),
            Node::Folder {
                name,
                path,
                children,
            } => {
                let key = format!("{prefix}/{path}");
                let open = c.is_open(&key);
                if folder_row(ui, depth, icon::FOLDER_SIMPLE, name, open, "").clicked() {
                    c.toggle(&key);
                }
                if open {
                    tree(ui, c, children, prefix, depth + 1, leaf);
                }
            }
        }
    }
}

/// What the user did in the sidebar.
pub enum Action {
    /// Select this commit.
    Jump(String),
    StashApply(String),
    StashPop(String),
    StashDrop(String),
    /// Check out a local branch.
    Checkout(String),
    /// Check out a remote branch ("origin/x") through a local tracking branch.
    CheckoutRemote(String),
    /// Open the new-branch dialog at `start` ("HEAD" or a commit id).
    NewBranch {
        start: String,
        label: String,
    },
    Rename(String),
    DeleteBranch(String),
    DeleteTag(String),
    /// Add a remote (`None`) or edit one.
    EditRemote(Option<String>),
    RemoveRemote(String),
    /// Rebase the checked-out branch onto this one.
    Rebase(String),
    /// Merge `source` into `target` (the checked-out branch if `None`).
    Merge {
        source: String,
        target: Option<String>,
    },
}

/// What's carried while dragging a branch (to drop it on another to merge).
#[derive(Clone)]
struct DragBranch(String);

/// Lets a row be dragged as `name`, and shows where it's being dragged.
fn drag_source(response: &egui::Response, name: &str) {
    response.dnd_set_drag_payload(DragBranch(name.to_string()));
}

/// Lets a local branch row take a dropped branch. Returns the merge asked for.
fn drop_target(ui: &egui::Ui, response: &egui::Response, name: &str) -> Option<Action> {
    if let Some(dragged) = response.dnd_hover_payload::<DragBranch>()
        && dragged.0 != name
    {
        ui.painter().rect_stroke(
            response.rect.shrink(1.0),
            4,
            Stroke::new(1.5, theme::accent()),
            egui::StrokeKind::Inside,
        );
    }
    let dropped = response.dnd_release_payload::<DragBranch>()?;
    (dropped.0 != name).then(|| Action::Merge {
        source: dropped.0.clone(),
        target: Some(name.to_string()),
    })
}

/// While a branch is dragged, a label follows the pointer.
fn drag_label(ctx: &egui::Context) {
    let (Some(dragged), Some(pos)) = (
        egui::DragAndDrop::payload::<DragBranch>(ctx),
        ctx.pointer_interact_pos(),
    ) else {
        return;
    };
    let painter = ctx.layer_painter(egui::LayerId::new(
        egui::Order::Tooltip,
        egui::Id::new("drag-branch"),
    ));
    let galley = painter.layout_no_wrap(
        format!("{}  Merge {}…", icon::GIT_MERGE, dragged.0),
        theme::body(12.0),
        theme::on_accent(),
    );
    let rect = Rect::from_min_size(pos + egui::vec2(14.0, 10.0), galley.size())
        .expand2(egui::vec2(8.0, 4.0));
    painter.rect_filled(rect, 6, theme::accent());
    painter.galley(rect.min + egui::vec2(8.0, 4.0), galley, theme::on_accent());
}

/// Adds the items of a context menu; the chosen one becomes the action.
fn menu(ui: &mut egui::Ui, action: &mut Option<Action>, items: Vec<(&str, bool, Action)>) {
    for (label, enabled, act) in items {
        if ui.add_enabled(enabled, egui::Button::new(label)).clicked() {
            *action = Some(act);
            ui.close();
        }
    }
}

/// Shows the sidebar.
pub fn show(
    ui: &mut egui::Ui,
    refs: &Refs,
    filter: &mut String,
    collapsed: &mut HashSet<String>,
    selected: Option<&str>,
) -> Option<Action> {
    ui.add_space(8.0);
    ui.horizontal(|ui| {
        ui.add_space(4.0);
        ui.add(
            egui::TextEdit::singleline(filter)
                .hint_text(format!("{}  Filter…", icon::FUNNEL_SIMPLE))
                .desired_width(ui.available_width() - 8.0),
        );
    });
    ui.add_space(4.0);
    ui.painter().hline(
        ui.max_rect().x_range(),
        ui.cursor().top(),
        Stroke::new(1.0, theme::border()),
    );

    let f = filter.trim().to_lowercase();
    let matches = |s: &str| f.is_empty() || s.to_lowercase().contains(&f);
    let local: Vec<&LocalBranch> = refs.local.iter().filter(|b| matches(&b.name)).collect();
    let remotes: Vec<(&str, Option<&str>, Vec<&RemoteBranch>)> = refs
        .remotes
        .iter()
        .map(|r| {
            (
                r.name.as_str(),
                r.url.as_deref(),
                r.branches
                    .iter()
                    .filter(|b| matches(&b.full_name))
                    .collect::<Vec<_>>(),
            )
        })
        .filter(|(name, _, branches)| f.is_empty() || !branches.is_empty() || matches(name))
        .collect();
    let head = refs
        .local
        .iter()
        .find(|b| b.is_head)
        .map(|b| b.name.as_str());
    let tags: Vec<_> = refs.tags.iter().filter(|t| matches(&t.name)).collect();
    let stashes: Vec<_> = refs
        .stashes
        .iter()
        .filter(|s| matches(&s.message) || matches(&format!("stash@{{{}}}", s.index)))
        .collect();

    let mut c = Ctx {
        collapsed,
        filtering: !f.is_empty(),
        selected,
        action: None,
    };
    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            let empty = |ui: &mut egui::Ui, text: &str| {
                ui.add_space(2.0);
                ui.horizontal(|ui| {
                    ui.add_space(28.0);
                    ui.label(RichText::new(text).size(12.0).color(theme::muted()));
                });
                ui.add_space(6.0);
            };
            let none = if f.is_empty() { "" } else { "No matches" };

            // Local branches.
            let open = c.is_open("local");
            let (toggled, add) = section_header(ui, "Local", local.len(), open, Some("New branch"));
            if toggled {
                c.toggle("local");
            }
            if add {
                let label = head.map_or_else(|| "HEAD".to_string(), |b| format!("{b} (HEAD)"));
                c.action = Some(Action::NewBranch {
                    start: "HEAD".into(),
                    label,
                });
            }
            if open {
                if local.is_empty() {
                    empty(ui, if none.is_empty() { "No branches" } else { none });
                }
                let nodes = build_tree(local.iter().copied(), |b: &LocalBranch| b.name.as_str());
                tree(
                    ui,
                    &mut c,
                    &nodes,
                    "local",
                    0,
                    &mut |ui, c, b: &LocalBranch, name, depth| {
                        let mut badges = Vec::new();
                        let tooltip = match &b.upstream {
                            Some(up) if up.gone => {
                                badges.push(Badge {
                                    text: "gone".into(),
                                    colour: theme::muted(),
                                });
                                format!("{}\nUpstream {} is gone", b.name, up.name)
                            }
                            Some(up) => {
                                if up.ahead > 0 {
                                    badges.push(Badge {
                                        text: format!("{} {}", icon::ARROW_UP, up.ahead),
                                        colour: theme::add(),
                                    });
                                }
                                if up.behind > 0 {
                                    badges.push(Badge {
                                        text: format!("{} {}", icon::ARROW_DOWN, up.behind),
                                        colour: theme::modified(),
                                    });
                                }
                                format!(
                                    "{}\nTracking {}: {} ahead, {} behind",
                                    b.name, up.name, up.ahead, up.behind
                                )
                            }
                            None => format!("{}\nNo upstream", b.name),
                        };
                        let glyph = if b.is_head {
                            icon::CHECK
                        } else {
                            icon::GIT_BRANCH
                        };
                        let response = row(
                            ui,
                            depth,
                            glyph,
                            name,
                            &badges,
                            c.selected == Some(b.target.as_str()),
                            b.is_head,
                            &tooltip,
                        );
                        if response.double_clicked() {
                            c.action = Some(Action::Checkout(b.name.clone()));
                        } else if response.clicked() {
                            c.action = Some(Action::Jump(b.target.clone()));
                        }
                        drag_source(&response, &b.name);
                        if let Some(a) = drop_target(ui, &response, &b.name) {
                            c.action = Some(a);
                        }
                        let merge_label = format!("Merge into {}", head.unwrap_or("HEAD"));
                        let rebase_label =
                            format!("Rebase {} onto {}", head.unwrap_or("HEAD"), b.name);
                        response.context_menu(|ui| {
                            let items = vec![
                                ("Check out", !b.is_head, Action::Checkout(b.name.clone())),
                                (
                                    merge_label.as_str(),
                                    !b.is_head && head.is_some(),
                                    Action::Merge {
                                        source: b.name.clone(),
                                        target: None,
                                    },
                                ),
                                (
                                    rebase_label.as_str(),
                                    !b.is_head && head.is_some(),
                                    Action::Rebase(b.name.clone()),
                                ),
                                (
                                    "Create branch here…",
                                    true,
                                    Action::NewBranch {
                                        start: b.target.clone(),
                                        label: b.name.clone(),
                                    },
                                ),
                                ("Rename…", true, Action::Rename(b.name.clone())),
                                ("Delete…", !b.is_head, Action::DeleteBranch(b.name.clone())),
                            ];
                            menu(ui, &mut c.action, items);
                        });
                    },
                );
            }

            // Remotes.
            let count = remotes.iter().map(|r| r.2.len()).sum();
            let open = c.is_open("remotes");
            let (toggled, add) = section_header(ui, "Remote", count, open, Some("Add remote"));
            if toggled {
                c.toggle("remotes");
            }
            if add {
                c.action = Some(Action::EditRemote(None));
            }
            if open {
                if remotes.is_empty() {
                    empty(ui, if none.is_empty() { "No remotes" } else { none });
                }
                for (name, url, branches) in &remotes {
                    let key = format!("remote:{name}");
                    let open = c.is_open(&key);
                    let response = folder_row(ui, 0, icon::CLOUD, name, open, url.unwrap_or(name));
                    if response.clicked() {
                        c.toggle(&key);
                    }
                    response.context_menu(|ui| {
                        let items = vec![
                            ("Edit…", true, Action::EditRemote(Some((*name).to_string()))),
                            ("Remove…", true, Action::RemoveRemote((*name).to_string())),
                        ];
                        menu(ui, &mut c.action, items);
                    });
                    if open {
                        let nodes = build_tree(branches.iter().copied(), |b: &RemoteBranch| {
                            b.name.as_str()
                        });
                        tree(
                            ui,
                            &mut c,
                            &nodes,
                            &key,
                            1,
                            &mut |ui, c, b: &RemoteBranch, name, depth| {
                                let response = row(
                                    ui,
                                    depth,
                                    icon::GIT_BRANCH,
                                    name,
                                    &[],
                                    c.selected == Some(b.target.as_str()),
                                    false,
                                    &b.full_name,
                                );
                                if response.double_clicked() {
                                    c.action = Some(Action::CheckoutRemote(b.full_name.clone()));
                                } else if response.clicked() {
                                    c.action = Some(Action::Jump(b.target.clone()));
                                }
                                drag_source(&response, &b.full_name);
                                let merge_label = format!("Merge into {}", head.unwrap_or("HEAD"));
                                let rebase_label = format!(
                                    "Rebase {} onto {}",
                                    head.unwrap_or("HEAD"),
                                    b.full_name
                                );
                                response.context_menu(|ui| {
                                    let items = vec![
                                        (
                                            merge_label.as_str(),
                                            head.is_some(),
                                            Action::Merge {
                                                source: b.full_name.clone(),
                                                target: None,
                                            },
                                        ),
                                        (
                                            rebase_label.as_str(),
                                            head.is_some(),
                                            Action::Rebase(b.full_name.clone()),
                                        ),
                                        (
                                            "Check out",
                                            true,
                                            Action::CheckoutRemote(b.full_name.clone()),
                                        ),
                                        (
                                            "Create branch here…",
                                            true,
                                            Action::NewBranch {
                                                start: b.target.clone(),
                                                label: b.full_name.clone(),
                                            },
                                        ),
                                    ];
                                    menu(ui, &mut c.action, items);
                                });
                            },
                        );
                    }
                }
            }

            // Tags.
            let open = c.is_open("tags");
            if section_header(ui, "Tags", tags.len(), open, None).0 {
                c.toggle("tags");
            }
            if open {
                if tags.is_empty() {
                    empty(ui, if none.is_empty() { "No tags" } else { none });
                }
                for t in &tags {
                    let tip = if t.annotated {
                        format!("{} (annotated)", t.name)
                    } else {
                        t.name.clone()
                    };
                    let response = row(
                        ui,
                        0,
                        icon::TAG,
                        &t.name,
                        &[],
                        c.selected == Some(t.target.as_str()),
                        false,
                        &tip,
                    );
                    if response.clicked() {
                        c.action = Some(Action::Jump(t.target.clone()));
                    }
                    response.context_menu(|ui| {
                        let items = vec![
                            (
                                "Create branch here…",
                                true,
                                Action::NewBranch {
                                    start: t.target.clone(),
                                    label: t.name.clone(),
                                },
                            ),
                            ("Delete tag…", true, Action::DeleteTag(t.name.clone())),
                        ];
                        menu(ui, &mut c.action, items);
                    });
                }
            }

            // Stashes.
            let open = c.is_open("stashes");
            if section_header(ui, "Stashes", stashes.len(), open, None).0 {
                c.toggle("stashes");
            }
            if open {
                if stashes.is_empty() {
                    empty(ui, if none.is_empty() { "No stashes" } else { none });
                }
                for s in &stashes {
                    let tip = format!("stash@{{{}}}: {}", s.index, s.message);
                    let response = row(
                        ui,
                        0,
                        icon::STACK,
                        &s.message,
                        &[],
                        c.selected == Some(s.target.as_str()),
                        false,
                        &tip,
                    );
                    if response.clicked() {
                        c.action = Some(Action::Jump(s.target.clone()));
                    }
                    response.context_menu(|ui| {
                        let items = vec![
                            ("Apply", true, Action::StashApply(s.target.clone())),
                            ("Pop", true, Action::StashPop(s.target.clone())),
                            ("Drop…", true, Action::StashDrop(s.target.clone())),
                        ];
                        menu(ui, &mut c.action, items);
                    });
                }
            }
            ui.add_space(12.0);
        });
    drag_label(ui.ctx());
    c.action
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shape(nodes: &[Node<'_, String>]) -> Vec<String> {
        nodes
            .iter()
            .map(|n| match n {
                Node::Folder { name, children, .. } => {
                    format!("{name}/[{}]", shape(children).join(","))
                }
                Node::Leaf { name, .. } => (*name).to_string(),
            })
            .collect()
    }

    #[test]
    fn groups_names_into_folders_folders_first() {
        let names: Vec<String> = [
            "main",
            "fix/ui/glitch",
            "feature/a",
            "fix/crash",
            "feature/b",
        ]
        .iter()
        .map(|s| (*s).to_string())
        .collect();
        let nodes = build_tree(names.iter(), String::as_str);
        assert_eq!(
            shape(&nodes),
            ["fix/[ui/[glitch],crash]", "feature/[a,b]", "main"]
        );
    }
}
