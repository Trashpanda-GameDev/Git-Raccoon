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
    // The checked-out branch: drawn as a filled label, as on the graph,
    // with a cloud when its upstream is at the same commit.
    head: Option<bool>,
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
    if head.is_none() {
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
    }

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
    if let Some(on_remote) = head {
        let mut label = format!("{} {text}", icon::CHECK);
        if on_remote {
            label.push_str("  ");
            label.push_str(icon::CLOUD);
        }
        widgets::pill(
            painter,
            Pos2::new(x - 2.0, rect.center().y),
            right - 4.0 - x,
            &label,
            theme::accent(),
            true,
        );
    } else {
        let label = Rect::from_min_max(
            Pos2::new(x + 20.0, rect.top()),
            Pos2::new(right - 4.0, rect.bottom()),
        );
        widgets::text_in(painter, label, text, font, colour);
    }
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
    /// Branches picked with Ctrl+click: "origin/x" for remote ones, which
    /// are kept apart by `Picked`.
    picked: Vec<Picked>,
}

/// A branch picked in the sidebar.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Picked {
    Local(String),
    Remote(String),
}

impl Ctx<'_> {
    /// A click on a branch's row: Ctrl (Cmd) picks it, or unpicks it;
    /// otherwise the picking is over. Returns whether it was picked.
    fn pick(&mut self, ui: &egui::Ui, response: &egui::Response, branch: Picked) -> bool {
        if !response.clicked() {
            return false;
        }
        if ui.input(|i| i.modifiers.command) {
            if let Some(at) = self.picked.iter().position(|p| *p == branch) {
                self.picked.remove(at);
            } else {
                self.picked.push(branch);
            }
            true
        } else {
            self.picked.clear();
            false
        }
    }

    fn is_picked(&self, branch: &Picked) -> bool {
        self.picked.contains(branch)
    }

    /// The context menu's bulk items, when the right-clicked branch is
    /// among several picked. Returns whether they were shown.
    fn picked_menu(&mut self, ui: &mut egui::Ui, branch: &Picked, head: Option<&str>) -> bool {
        if self.picked.len() < 2 || !self.is_picked(branch) {
            return false;
        }
        let local: Vec<String> = self
            .picked
            .iter()
            .filter_map(|p| match p {
                Picked::Local(n) if Some(n.as_str()) != head => Some(n.clone()),
                _ => None,
            })
            .collect();
        let remote: Vec<String> = self
            .picked
            .iter()
            .filter_map(|p| match p {
                Picked::Remote(n) => Some(n.clone()),
                Picked::Local(_) => None,
            })
            .collect();
        if !local.is_empty() {
            let label = match local.len() {
                1 => format!("Delete {}…", local[0]),
                n => format!("Delete {n} branches…"),
            };
            if ui.button(label).clicked() {
                self.action = Some(if local.len() == 1 {
                    Action::DeleteBranch(local[0].clone())
                } else {
                    Action::DeleteBranches(local)
                });
                self.picked.clear();
                ui.close();
            }
        }
        if !remote.is_empty() {
            let label = match remote.len() {
                1 => format!("Delete {} on the remote…", remote[0]),
                n => format!("Delete {n} on the remote…"),
            };
            if ui.button(label).clicked() {
                self.action = Some(if remote.len() == 1 {
                    Action::DeleteRemoteBranch(remote[0].clone())
                } else {
                    Action::DeleteRemoteBranches(remote)
                });
                self.picked.clear();
                ui.close();
            }
        }
        if ui.button("Unpick all").clicked() {
            self.picked.clear();
            ui.close();
        }
        true
    }

    fn is_open(&self, key: &str) -> bool {
        self.filtering || !self.collapsed.contains(key)
    }

    fn toggle(&mut self, key: &str) {
        if !self.collapsed.remove(key) {
            self.collapsed.insert(key.to_string());
        }
    }
}

/// The pull/merge requests and issues sections.
fn forge_sections(
    ui: &mut egui::Ui,
    c: &mut Ctx<'_>,
    forge: &crate::forge_view::ForgeView,
    matches: &dyn Fn(&str) -> bool,
) {
    use crate::forge_view::Opened;
    let kind = forge.project.kind;
    let note = |ui: &mut egui::Ui, text: &str| {
        ui.add_space(2.0);
        ui.horizontal(|ui| {
            ui.add_space(28.0);
            ui.label(RichText::new(text).size(12.0).color(theme::muted()));
        });
        ui.add_space(6.0);
    };
    // Before signing in: one link, for both sections.
    let signed_in = forge.account.is_some();
    let requests: Vec<&crate::forge::Request> = forge
        .requests
        .as_ref()
        .and_then(|r| r.as_ref().ok())
        .into_iter()
        .flatten()
        .filter(|r| matches(&r.title) || matches(&kind.number(r.number)))
        .collect();
    let title = format!("{}s", kind.request());
    let open = c.is_open("requests");
    let (toggled, add) = section_header(
        ui,
        &title,
        requests.len(),
        open,
        signed_in.then_some(match kind {
            crate::forge::Kind::GitHub => "New pull request",
            crate::forge::Kind::GitLab => "New merge request",
        }),
    );
    if toggled {
        c.toggle("requests");
    }
    if add {
        c.action = Some(Action::NewRequest);
    }
    let sign_in = |ui: &mut egui::Ui, c: &mut Ctx<'_>| {
        ui.horizontal(|ui| {
            ui.add_space(28.0);
            let link = ui.link(
                RichText::new(format!(
                    "Sign in to {} to see them",
                    forge.project.kind.name()
                ))
                .size(12.0),
            );
            if link.clicked() {
                c.action = Some(Action::SignInToForge);
            }
        });
        ui.add_space(6.0);
    };
    if open {
        match &forge.requests {
            _ if !signed_in => sign_in(ui, c),
            None => note(ui, "Loading…"),
            Some(Err(e)) => note(ui, e),
            Some(Ok(_)) if requests.is_empty() => {
                note(ui, &format!("No open {}", title.to_lowercase()))
            }
            Some(Ok(_)) => {
                for r in requests {
                    let badges: Vec<Badge> = r
                        .draft
                        .then(|| Badge {
                            text: "draft".into(),
                            colour: theme::muted(),
                        })
                        .into_iter()
                        .collect();
                    let tip = format!("{}\n{} → {} by {}", r.title, r.source, r.target, r.author);
                    let response = row(
                        ui,
                        0,
                        icon::GIT_PULL_REQUEST,
                        &format!("{} {}", kind.number(r.number), r.title),
                        &badges,
                        forge.open == Some(Opened::Request(r.number)),
                        false,
                        &tip,
                        None,
                    );
                    if response.clicked() {
                        c.action = Some(Action::OpenRequest(r.number));
                    }
                    let page = (format!("Open on {}", kind.name()), r.url.clone());
                    response.context_menu(|ui| {
                        web_items(ui, &mut c.action, Some(&page));
                        if ui.button("Show here").clicked() {
                            c.action = Some(Action::OpenRequest(r.number));
                            ui.close();
                        }
                    });
                }
            }
        }
    }
    let issues: Vec<&crate::forge::Issue> = forge
        .issues
        .as_ref()
        .and_then(|r| r.as_ref().ok())
        .into_iter()
        .flatten()
        .filter(|i| matches(&i.title) || matches(&format!("#{}", i.number)))
        .collect();
    let open = c.is_open("issues");
    if section_header(ui, "Issues", issues.len(), open, None).0 {
        c.toggle("issues");
    }
    if open {
        match &forge.issues {
            _ if !signed_in => sign_in(ui, c),
            None => note(ui, "Loading…"),
            Some(Err(e)) => note(ui, e),
            Some(Ok(_)) if issues.is_empty() => note(ui, "No open issues"),
            Some(Ok(_)) => {
                for i in issues {
                    let tip = format!("{}\nby {}", i.title, i.author);
                    let response = row(
                        ui,
                        0,
                        icon::CIRCLE_DASHED,
                        &format!("#{} {}", i.number, i.title),
                        &[],
                        forge.open == Some(Opened::Issue(i.number)),
                        false,
                        &tip,
                        None,
                    );
                    if response.clicked() {
                        c.action = Some(Action::OpenIssue(i.number));
                    }
                    let page = (
                        format!("Open on {}", forge.project.kind.name()),
                        i.url.clone(),
                    );
                    response.context_menu(|ui| {
                        web_items(ui, &mut c.action, Some(&page));
                        if ui.button("Show here").clicked() {
                            c.action = Some(Action::OpenIssue(i.number));
                            ui.close();
                        }
                    });
                }
            }
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
    /// Show a pull/merge request, or an issue, in the details pane.
    OpenRequest(u64),
    OpenIssue(u64),
    /// Sign in to the repository's forge.
    SignInToForge,
    /// Open a new pull/merge request.
    NewRequest,
    /// Open a page on the forge.
    OpenWeb(String),
    StashApply(String),
    StashPop(String),
    StashDrop(String),
    /// Check out a local branch.
    Checkout(String),
    /// Check out a remote branch ("origin/x") through a local tracking branch.
    CheckoutRemote(String),
    /// Delete a branch on its remote ("origin/feature"), after asking.
    DeleteRemoteBranch(String),
    /// Delete several local branches, or several on their remotes.
    DeleteBranches(Vec<String>),
    DeleteRemoteBranches(Vec<String>),
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
    /// Open this repository (a submodule or worktree) in a tab.
    OpenPath(String),
    /// Update the submodule at this path (relative to the repository).
    UpdateSubmodule(String),
    AddWorktree,
    RemoveWorktree(String),
    /// Merge `source` into `target` (the checked-out branch if `None`).
    Merge {
        source: String,
        target: Option<String>,
    },
}

/// What's carried while dragging a branch (to drop it on another to merge):
/// from the sidebar or from a label on the graph, onto either.
#[derive(Clone)]
struct DragBranch(String);

/// Lets a row or label be dragged as the branch `name` ("origin/x" for a
/// remote one).
pub fn drag_source(response: &egui::Response, name: &str) {
    response.dnd_set_drag_payload(DragBranch(name.to_string()));
}

/// Lets a local branch's row or label take a dropped branch: outlines it
/// while one is dragged over it, and returns the one dropped on it.
pub fn dropped_branch(
    painter: &egui::Painter,
    response: &egui::Response,
    name: &str,
) -> Option<String> {
    if let Some(dragged) = response.dnd_hover_payload::<DragBranch>()
        && dragged.0 != name
    {
        painter.rect_stroke(
            response.rect.shrink(1.0),
            4,
            Stroke::new(1.5, theme::accent()),
            egui::StrokeKind::Inside,
        );
    }
    let dropped = response.dnd_release_payload::<DragBranch>()?;
    (dropped.0 != name).then(|| dropped.0.clone())
}

/// Lets a local branch row take a dropped branch. Returns the merge asked for.
fn drop_target(ui: &egui::Ui, response: &egui::Response, name: &str) -> Option<Action> {
    dropped_branch(ui.painter(), response, name).map(|source| Action::Merge {
        source,
        target: Some(name.to_string()),
    })
}

/// While a branch is dragged, a label follows the pointer.
pub fn drag_label(ctx: &egui::Context) {
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
/// "Open on GitHub" (and copying the link) at the top of a context menu.
fn web_items(ui: &mut egui::Ui, action: &mut Option<Action>, web: Option<&(String, String)>) {
    let Some((text, url)) = web else { return };
    if ui
        .button(format!("{}  {text}", icon::ARROW_SQUARE_OUT))
        .clicked()
    {
        *action = Some(Action::OpenWeb(url.clone()));
        ui.close();
    }
    if ui
        .button(format!("{}  Copy its link", icon::LINK))
        .clicked()
    {
        ui.ctx().copy_text(url.clone());
        ui.close();
    }
    ui.separator();
}

fn menu(ui: &mut egui::Ui, action: &mut Option<Action>, items: Vec<(&str, bool, Action)>) {
    for (label, enabled, act) in items {
        if ui.add_enabled(enabled, egui::Button::new(label)).clicked() {
            *action = Some(act);
            ui.close();
        }
    }
}

/// Shows the sidebar.
/// What's shown as picked, and the forge's sections.
pub struct Shown<'a> {
    /// The selected commit.
    pub selected: Option<&'a str>,
    pub forge: Option<&'a crate::forge_view::ForgeView>,
    /// The remotes' web pages.
    pub web: &'a crate::forge::Web,
}

pub fn show(
    ui: &mut egui::Ui,
    refs: &Refs,
    extras: &crate::repo_view::Extras,
    root: &str,
    filter: &mut String,
    collapsed: &mut HashSet<String>,
    shown: Shown<'_>,
) -> Option<Action> {
    let Shown {
        selected,
        forge,
        web,
    } = shown;
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

    let picked_id = egui::Id::new(("sidebar-picked", root));
    let mut c = Ctx {
        collapsed,
        filtering: !f.is_empty(),
        selected,
        action: None,
        picked: ui.data(|d| d.get_temp::<Vec<Picked>>(picked_id).unwrap_or_default()),
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
                            c.selected == Some(b.target.as_str())
                                || c.is_picked(&Picked::Local(b.name.clone())),
                            b.is_head,
                            &tooltip,
                            b.is_head.then(|| {
                                b.upstream
                                    .as_ref()
                                    .is_some_and(|u| !u.gone && u.ahead == 0 && u.behind == 0)
                            }),
                        );
                        let me = Picked::Local(b.name.clone());
                        if response.double_clicked() {
                            c.action = Some(Action::Checkout(b.name.clone()));
                        } else if c.pick(ui, &response, me.clone()) {
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
                        // On the forge, as its upstream.
                        let page = b
                            .upstream
                            .as_ref()
                            .filter(|u| !u.gone)
                            .and_then(|u| web.branch(&u.name));
                        response.context_menu(|ui| {
                            if c.picked_menu(ui, &me, head) {
                                return;
                            }
                            web_items(ui, &mut c.action, page.as_ref());
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
                    let page = web.remote_page(name);
                    response.context_menu(|ui| {
                        web_items(ui, &mut c.action, page.as_ref());
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
                                    c.selected == Some(b.target.as_str())
                                        || c.is_picked(&Picked::Remote(b.full_name.clone())),
                                    false,
                                    &b.full_name,
                                    None,
                                );
                                let me = Picked::Remote(b.full_name.clone());
                                if response.double_clicked() {
                                    c.action = Some(Action::CheckoutRemote(b.full_name.clone()));
                                } else if c.pick(ui, &response, me.clone()) {
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
                                let page = web.branch(&b.full_name);
                                response.context_menu(|ui| {
                                    if c.picked_menu(ui, &me, head) {
                                        return;
                                    }
                                    web_items(ui, &mut c.action, page.as_ref());
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
                                        (
                                            "Delete on the remote…",
                                            true,
                                            Action::DeleteRemoteBranch(b.full_name.clone()),
                                        ),
                                    ];
                                    menu(ui, &mut c.action, items);
                                });
                            },
                        );
                    }
                }
            }

            // The forge's pull/merge requests and issues.
            if let Some(forge) = forge {
                forge_sections(ui, &mut c, forge, &matches);
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
                        None,
                    );
                    if response.clicked() {
                        c.action = Some(Action::Jump(t.target.clone()));
                    }
                    let page = web.tag(&t.name);
                    response.context_menu(|ui| {
                        web_items(ui, &mut c.action, page.as_ref());
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
                        None,
                    );
                    if response.clicked() {
                        c.action = Some(Action::Jump(s.target.clone()));
                    }
                    response.context_menu(|ui| {
                        let items = vec![
                            ("Apply", true, Action::StashApply(s.target.clone())),
                            ("Pop", true, Action::StashPop(s.target.clone())),
                            ("Delete…", true, Action::StashDrop(s.target.clone())),
                        ];
                        menu(ui, &mut c.action, items);
                    });
                }
            }
            // Worktrees.
            let open = c.is_open("worktrees");
            let (toggled, add) = section_header(
                ui,
                "Worktrees",
                extras.worktrees.len(),
                open,
                Some("Add worktree"),
            );
            if toggled {
                c.toggle("worktrees");
            }
            if add {
                c.action = Some(Action::AddWorktree);
            }
            if open {
                for w in &extras.worktrees {
                    let folder = std::path::Path::new(&w.path)
                        .file_name()
                        .map_or_else(|| w.path.clone(), |n| n.to_string_lossy().into_owned());
                    let branch = w.branch.clone().unwrap_or_else(|| "detached".into());
                    let mut badges = vec![Badge {
                        text: branch,
                        colour: theme::muted(),
                    }];
                    if w.missing {
                        badges.push(Badge {
                            text: "missing".into(),
                            colour: theme::danger(),
                        });
                    }
                    let tip = format!(
                        "{}{}",
                        w.path,
                        if w.current { "\nThis worktree" } else { "" }
                    );
                    let glyph = if w.current {
                        icon::CHECK
                    } else {
                        icon::FOLDER_SIMPLE
                    };
                    let response =
                        row(ui, 0, glyph, &folder, &badges, false, w.current, &tip, None);
                    if response.double_clicked() && !w.current {
                        c.action = Some(Action::OpenPath(w.path.clone()));
                    }
                    response.context_menu(|ui| {
                        let items = vec![
                            (
                                "Open in a tab",
                                !w.current && !w.missing,
                                Action::OpenPath(w.path.clone()),
                            ),
                            (
                                "Remove…",
                                !w.main && !w.current,
                                Action::RemoveWorktree(w.path.clone()),
                            ),
                        ];
                        menu(ui, &mut c.action, items);
                    });
                }
            }

            // Submodules, if there are any.
            if !extras.submodules.is_empty() {
                let open = c.is_open("submodules");
                if section_header(ui, "Submodules", extras.submodules.len(), open, None).0 {
                    c.toggle("submodules");
                }
                if open {
                    use git_raccoon_core::modules::SubmoduleState;
                    for s in &extras.submodules {
                        let (badge, colour, about) = match s.state {
                            SubmoduleState::UpToDate => ("", theme::muted(), "Up to date"),
                            SubmoduleState::Uninitialized => (
                                "not cloned",
                                theme::modified(),
                                "Not cloned yet: update it to get its files",
                            ),
                            SubmoduleState::OtherCommit => (
                                "other commit",
                                theme::modified(),
                                "A different commit is checked out than the one recorded here",
                            ),
                            SubmoduleState::Changed => {
                                ("changed", theme::modified(), "It has uncommitted changes")
                            }
                        };
                        let badges: Vec<Badge> = if badge.is_empty() {
                            Vec::new()
                        } else {
                            vec![Badge {
                                text: badge.into(),
                                colour,
                            }]
                        };
                        let tip = format!(
                            "{}\n{}{}",
                            s.path,
                            about,
                            s.url
                                .as_deref()
                                .map(|u| format!("\n{u}"))
                                .unwrap_or_default()
                        );
                        let full = std::path::Path::new(root)
                            .join(&s.path)
                            .to_string_lossy()
                            .into_owned();
                        let cloned = s.state != SubmoduleState::Uninitialized;
                        let response = row(
                            ui,
                            0,
                            icon::PACKAGE,
                            &s.path,
                            &badges,
                            false,
                            false,
                            &tip,
                            None,
                        );
                        if response.double_clicked() && cloned {
                            c.action = Some(Action::OpenPath(full.clone()));
                        }
                        response.context_menu(|ui| {
                            let items = vec![
                                ("Open in a tab", cloned, Action::OpenPath(full.clone())),
                                (
                                    if cloned { "Update" } else { "Clone and update" },
                                    true,
                                    Action::UpdateSubmodule(s.path.clone()),
                                ),
                            ];
                            menu(ui, &mut c.action, items);
                        });
                    }
                }
            }
            ui.add_space(12.0);
        });
    // Esc unpicks.
    if !c.picked.is_empty() && ui.input(|i| i.key_pressed(egui::Key::Escape)) {
        c.picked.clear();
    }
    ui.data_mut(|d| d.insert_temp(picked_id, c.picked.clone()));
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
