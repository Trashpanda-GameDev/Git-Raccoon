//! The left sidebar: local branches, remotes, tags and stashes.

use std::collections::{HashMap, HashSet};

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
    c: &mut Ctx<'_>,
    key: &str,
    title: &str,
    count: usize,
    open: bool,
    add: Option<&str>,
) -> (bool, bool) {
    // Drawn in its turn: open ones first, collapsed ones at the bottom.
    if !c.now(key) {
        return (false, false);
    }
    let (rect, response) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), HEADER), Sense::click());
    // Its top edge, so the sections read as separate; an open one after
    // another has the handle's line above it already.
    let after_handle = open
        && c.sizes
            .open
            .iter()
            .position(|k| k == key)
            .is_some_and(|at| at > 0);
    if !after_handle {
        ui.painter().hline(
            rect.x_range(),
            rect.top(),
            Stroke::new(1.0, theme::border()),
        );
    }
    // Which sections to show.
    response.context_menu(|ui| {
        ui.label(
            RichText::new("Show in the sidebar")
                .size(12.0)
                .color(theme::muted()),
        );
        for (k, name) in &c.all_sections {
            let mut shown = !c.hidden.iter().any(|h| h == k);
            if ui.checkbox(&mut shown, name).clicked() {
                c.action = Some(Action::ToggleSection((*k).to_string()));
                ui.close();
            }
        }
        if !c.hidden.is_empty() && ui.button("Show all sections").clicked() {
            c.action = Some(Action::ShowAllSections);
            ui.close();
        }
    });
    response
        .widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::CollapsingHeader, true, title));
    let mut added = false;
    if let Some(name) = add {
        let r = Rect::from_center_size(
            Pos2::new(rect.right() - 40.0, rect.center().y + 2.0),
            egui::vec2(20.0, 20.0),
        );
        // Placed, so it doesn't move what comes after.
        let button = ui.place(
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

/// Every section the sidebar can have (key, name), in order; some only
/// show when there's something for them (a forge, submodules).
pub const SECTIONS: &[(&str, &str)] = &[
    ("local", "Local branches"),
    ("remotes", "Remote branches"),
    ("requests", "Pull or merge requests"),
    ("issues", "Issues"),
    ("tags", "Tags"),
    ("stashes", "Stashes"),
    ("worktrees", "Worktrees"),
    ("submodules", "Submodules"),
];

/// How tall each open section is. The open sections always share all
/// the height there is (#44), each by its share; a handle between two
/// moves just the line between them (pushing past a neighbour at its
/// smallest to the next). Shares are kept between runs; without any, a
/// section's share is what's in it, and the first one (Local) takes the
/// rest.
#[derive(Default)]
struct Sizes {
    /// The height each open section gets this frame.
    heights: HashMap<String, f32>,
    /// How tall each one's contents were last frame.
    measured: HashMap<String, f32>,
    /// Each section's share (in points, as it was when last set).
    shares: HashMap<String, f32>,
    /// The open sections, in order.
    open: Vec<String>,
}

/// Space between two open sections, to drag.
const HANDLE: f32 = 6.0;
const HEADER: f32 = 28.0;
/// The smallest an open section's list gets.
const SMALLEST: f32 = 44.0;

impl Sizes {
    /// Shares `space` among the open sections (in order).
    fn plan(&mut self, open: &[&str], space: f32) {
        self.open = open.iter().map(ToString::to_string).collect();
        self.heights.clear();
        if open.is_empty() {
            return;
        }
        let content = |k: &str| self.measured.get(k).copied().unwrap_or(120.0).max(SMALLEST);
        let shared: Vec<f32> = if open.iter().any(|k| self.shares.contains_key(*k)) {
            open.iter()
                .map(|k| self.shares.get(*k).copied().unwrap_or_else(|| content(k)))
                .collect()
        } else {
            // What's in each; the first takes what's left.
            let rest: f32 = open[1..].iter().map(|k| content(k)).sum();
            std::iter::once((space - rest).max(SMALLEST))
                .chain(open[1..].iter().map(|k| content(k)))
                .collect()
        };
        let total: f32 = shared.iter().sum::<f32>().max(1.0);
        let mut heights: Vec<f32> = shared.iter().map(|w| w / total * space).collect();
        fit(&mut heights, space);
        self.heights = open.iter().map(ToString::to_string).zip(heights).collect();
    }

    /// Heights of the open sections, in order.
    fn ordered(&self) -> Vec<f32> {
        self.open
            .iter()
            .map(|k| self.heights.get(k).copied().unwrap_or(0.0))
            .collect()
    }

    /// Moves the line under open section `at` by `dy`: it grows (or
    /// shrinks) and the next (or itself, and the ones before) give way,
    /// none below their smallest. The others stay as they are.
    fn drag(&mut self, at: usize, dy: f32) {
        let mut heights = self.ordered();
        if at + 1 >= heights.len() {
            return;
        }
        if dy > 0.0 {
            let mut want = dy;
            for h in heights.iter_mut().skip(at + 1) {
                let give = (*h - SMALLEST).max(0.0).min(want);
                *h -= give;
                want -= give;
            }
            heights[at] += dy - want;
        } else {
            let mut want = -dy;
            for h in heights[..=at].iter_mut().rev() {
                let give = (*h - SMALLEST).max(0.0).min(want);
                *h -= give;
                want -= give;
            }
            heights[at + 1] += -dy - want;
        }
        for (k, h) in self.open.iter().zip(&heights) {
            self.shares.insert(k.clone(), *h);
            self.heights.insert(k.clone(), *h);
        }
    }
}

/// Keeps every height at least the smallest (where there's room), taking
/// what that needs from the taller ones; they still add up to `space`.
fn fit(heights: &mut [f32], space: f32) {
    #[expect(clippy::cast_precision_loss, reason = "a few sections")]
    let n = heights.len() as f32;
    if space < SMALLEST * n {
        heights.iter_mut().for_each(|h| *h = space / n);
        return;
    }
    for _ in 0..4 {
        let short: f32 = heights.iter().map(|h| (SMALLEST - h).max(0.0)).sum();
        if short <= 0.0 {
            break;
        }
        let spare: f32 = heights.iter().map(|h| (h - SMALLEST).max(0.0)).sum();
        for h in heights.iter_mut() {
            if *h < SMALLEST {
                *h = SMALLEST;
            } else if spare > 0.0 {
                *h -= (*h - SMALLEST) / spare * short;
            }
        }
    }
}

/// An open section's contents, in their own scrolling space, and the
/// handle under them for resizing it.
fn body(
    ui: &mut egui::Ui,
    c: &mut Ctx<'_>,
    key: &str,
    title: &str,
    add: impl FnOnce(&mut egui::Ui, &mut Ctx<'_>),
) {
    if !c.now(key) {
        return;
    }
    let height = c.sizes.heights.get(key).copied().unwrap_or(SMALLEST);
    let width = ui.available_width();
    let shown = ui
        .allocate_ui(egui::vec2(width, height), |ui| {
            egui::ScrollArea::vertical()
                .id_salt(("sidebar-section", key))
                .auto_shrink([false, false])
                .max_height(height)
                .min_scrolled_height(height)
                .show(ui, |ui| {
                    ui.spacing_mut().item_spacing.y = 0.0;
                    ui.set_max_width(width);
                    // What's drawn, not the space it's in.
                    let top = ui.cursor().top();
                    add(ui, c);
                    ui.cursor().top() - top
                })
        })
        .inner;
    c.sizes.measured.insert(key.to_string(), shown.inner);
    // The handle between this section and the next open one.
    let Some(at) = c.sizes.open.iter().position(|k| k == key) else {
        return;
    };
    if at + 1 >= c.sizes.open.len() {
        return;
    }
    let (rect, handle) = ui.allocate_exact_size(egui::vec2(width, HANDLE), Sense::click_and_drag());
    handle.widget_info(|| {
        egui::WidgetInfo::labeled(egui::WidgetType::Other, true, format!("Resize {title}"))
    });
    let handle = handle
        .on_hover_cursor(egui::CursorIcon::ResizeVertical)
        .on_hover_text("Drag to share the space; double-click to share it by what's in each");
    let (thickness, colour) = if handle.hovered() || handle.dragged() {
        (2.0, theme::accent())
    } else {
        (1.0, theme::border())
    };
    ui.painter().hline(
        rect.x_range(),
        rect.center().y,
        Stroke::new(thickness, colour),
    );
    if handle.double_clicked() {
        c.sizes.shares.clear();
    } else if handle.dragged() {
        c.sizes.drag(at, handle.drag_delta().y);
    }
}

struct Ctx<'s> {
    sizes: Sizes,
    /// The sections there are (key, name), whether shown or not, and the
    /// ones hidden.
    all_sections: Vec<(&'static str, String)>,
    hidden: &'s [String],
    /// Drawing the open sections (then the collapsed ones, at the bottom).
    open_pass: bool,
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
    /// Whether a section is drawn now: it's shown, and it's open in the
    /// first pass, collapsed in the second.
    fn now(&self, key: &str) -> bool {
        self.all_sections.iter().any(|(k, _)| *k == key)
            && !self.hidden.iter().any(|h| h == key)
            && self.is_open(key) == self.open_pass
    }

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
        // Wrapped, so a long note doesn't widen the sidebar.
        egui::Frame::new()
            .inner_margin(egui::Margin {
                left: 28,
                right: 4,
                ..egui::Margin::ZERO
            })
            .show(ui, |ui| {
                ui.add(
                    egui::Label::new(RichText::new(text).size(12.0).color(theme::muted())).wrap(),
                );
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
        c,
        "requests",
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
        body(ui, c, "requests", &title, |ui, c| match &forge.requests {
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
        });
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
    if section_header(ui, c, "issues", "Issues", issues.len(), open, None).0 {
        c.toggle("issues");
    }
    if open {
        body(ui, c, "issues", "Issues", |ui, c| match &forge.issues {
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
        });
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
    /// Show or hide a section of the sidebar (by its key).
    ToggleSection(String),
    ShowAllSections,
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
    /// Choose the remote branch a branch pulls from and pushes to.
    SetUpstream(String),
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
    /// Open a terminal in this folder (a worktree or submodule).
    Terminal(String),
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
    painter.rect_filled(rect, 6, theme::accent_fill());
    painter.galley(rect.min + egui::vec2(8.0, 4.0), galley, theme::on_accent());
}

/// "Copy name" in a branch's or tag's menu: puts its name on the
/// clipboard ("origin/feature" for a remote branch).
pub fn copy_name(ui: &mut egui::Ui, name: &str) {
    let item = ui.button(format!("{}  Copy name", icon::COPY));
    ui.ctx()
        .accesskit_node_builder(item.id, |n| n.set_label("Copy name"));
    if item.on_hover_text(name).clicked() {
        ui.ctx().copy_text(name.to_string());
        ui.close();
    }
}

/// A page on the forge, and copying its link, at the top of a menu.
pub fn web_items(ui: &mut egui::Ui, action: &mut Option<Action>, web: Option<&(String, String)>) {
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

pub fn menu<S: AsRef<str>>(
    ui: &mut egui::Ui,
    action: &mut Option<Action>,
    items: Vec<(S, bool, Action)>,
) {
    for (label, enabled, act) in items {
        if ui
            .add_enabled(enabled, egui::Button::new(label.as_ref()))
            .clicked()
        {
            *action = Some(act);
            ui.close();
        }
    }
}

/// What can be done with a local branch at `target`, from its right-click
/// menu (here or on the graph). `head`: the checked-out branch.
pub fn local_branch_items(
    name: &str,
    target: &str,
    is_head: bool,
    head: Option<&str>,
) -> Vec<(String, bool, Action)> {
    let onto = head.unwrap_or("HEAD");
    vec![
        // Named, so it isn't taken for checking out just its commit.
        (
            format!("Check out {name}"),
            !is_head,
            Action::Checkout(name.into()),
        ),
        (
            format!("Merge into {onto}"),
            !is_head && head.is_some(),
            Action::Merge {
                source: name.into(),
                target: None,
            },
        ),
        (
            format!("Rebase {onto} onto {name}"),
            !is_head && head.is_some(),
            Action::Rebase(name.into()),
        ),
        (
            "Create branch here…".into(),
            true,
            Action::NewBranch {
                start: target.into(),
                label: name.into(),
            },
        ),
        ("Rename…".into(), true, Action::Rename(name.into())),
        (
            "Set upstream…".into(),
            true,
            Action::SetUpstream(name.into()),
        ),
        (
            "Delete…".into(),
            !is_head,
            Action::DeleteBranch(name.into()),
        ),
    ]
}

/// What can be done with a remote branch ("origin/x") at `target`.
pub fn remote_branch_items(
    full_name: &str,
    target: &str,
    head: Option<&str>,
) -> Vec<(String, bool, Action)> {
    let onto = head.unwrap_or("HEAD");
    // Checking it out makes (or brings up to date) the local branch.
    let local = crate::repo_view::local_name(full_name);
    vec![
        (
            format!("Merge into {onto}"),
            head.is_some(),
            Action::Merge {
                source: full_name.into(),
                target: None,
            },
        ),
        (
            format!("Rebase {onto} onto {full_name}"),
            head.is_some(),
            Action::Rebase(full_name.into()),
        ),
        (
            format!("Check out as {local}"),
            true,
            Action::CheckoutRemote(full_name.into()),
        ),
        (
            "Create branch here…".into(),
            true,
            Action::NewBranch {
                start: target.into(),
                label: full_name.into(),
            },
        ),
        (
            "Delete on the remote…".into(),
            true,
            Action::DeleteRemoteBranch(full_name.into()),
        ),
    ]
}

/// What can be done with a tag at `target`.
pub fn tag_items(name: &str, target: &str) -> Vec<(String, bool, Action)> {
    vec![
        (
            "Create branch here…".into(),
            true,
            Action::NewBranch {
                start: target.into(),
                label: name.into(),
            },
        ),
        ("Delete tag…".into(), true, Action::DeleteTag(name.into())),
    ]
}

/// Shows the sidebar.
/// What's shown as picked, and the forge's sections.
pub struct Shown<'a> {
    /// The selected commit.
    pub selected: Option<&'a str>,
    pub forge: Option<&'a crate::forge_view::ForgeView>,
    /// The remotes' web pages.
    pub web: &'a crate::forge::Web,
    /// Sections hidden (by key).
    pub hidden: &'a [String],
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
        hidden,
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
    // The sections there are, with their names.
    let filtering = !f.is_empty();
    let mut all_sections: Vec<(&'static str, String)> =
        vec![("local", "Local".into()), ("remotes", "Remote".into())];
    if let Some(forge) = forge {
        all_sections.push(("requests", format!("{}s", forge.project.kind.request())));
        all_sections.push(("issues", "Issues".into()));
    }
    all_sections.extend([
        ("tags", "Tags".into()),
        ("stashes", "Stashes".into()),
        ("worktrees", "Worktrees".into()),
    ]);
    if !extras.submodules.is_empty() {
        all_sections.push(("submodules", "Submodules".into()));
    }
    let shown_keys: Vec<&str> = all_sections
        .iter()
        .map(|(k, _)| *k)
        .filter(|k| !hidden.iter().any(|h| h == k))
        .collect();
    let open: Vec<&str> = shown_keys
        .iter()
        .copied()
        .filter(|k| filtering || !collapsed.contains(*k))
        .collect();
    let (shares_id, measured_id) = (
        egui::Id::new("sidebar-section-shares"),
        egui::Id::new(("sidebar-section-contents", root)),
    );
    let mut sizes = Sizes {
        shares: ui.data_mut(|d| {
            d.get_persisted::<HashMap<String, f32>>(shares_id)
                .unwrap_or_default()
        }),
        measured: ui.data(|d| {
            d.get_temp::<HashMap<String, f32>>(measured_id)
                .unwrap_or_default()
        }),
        ..Sizes::default()
    };
    #[expect(clippy::cast_precision_loss, reason = "a few sections")]
    let chrome = HEADER * shown_keys.len() as f32 + HANDLE * open.len().saturating_sub(1) as f32;
    sizes.plan(&open, (ui.available_height() - chrome).max(0.0));
    let collapsed_shown = shown_keys.len() - open.len();
    let mut c = Ctx {
        sizes,
        all_sections,
        hidden,
        open_pass: true,
        collapsed,
        filtering: !f.is_empty(),
        selected,
        action: None,
        picked: ui.data(|d| d.get_temp::<Vec<Picked>>(picked_id).unwrap_or_default()),
    };
    // Not one scrolling list: each section scrolls on its own (#13).
    ui.scope(|ui| {
        ui.spacing_mut().item_spacing.y = 0.0;
        let empty = |ui: &mut egui::Ui, text: &str| {
            ui.add_space(2.0);
            // Wrapped, so a long note doesn't widen the sidebar.
            egui::Frame::new()
                .inner_margin(egui::Margin {
                    left: 28,
                    right: 4,
                    ..egui::Margin::ZERO
                })
                .show(ui, |ui| {
                    ui.add(
                        egui::Label::new(RichText::new(text).size(12.0).color(theme::muted()))
                            .wrap(),
                    );
                });
            ui.add_space(6.0);
        };
        let none = if f.is_empty() { "" } else { "No matches" };
        // Open sections, then the collapsed ones at the bottom.
        for open_pass in [true, false] {
            c.open_pass = open_pass;
            if !open_pass {
                #[expect(clippy::cast_precision_loss, reason = "a few sections")]
                let below = ui.available_height() - HEADER * collapsed_shown as f32;
                if below > 0.0 {
                    ui.add_space(below);
                }
            }

            // Local branches.
            let open = c.is_open("local");
            let (toggled, add) = section_header(
                ui,
                &mut c,
                "local",
                "Local",
                local.len(),
                open,
                Some("New branch"),
            );
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
                body(ui, &mut c, "local", "Local", |ui, c| {
                    if local.is_empty() {
                        empty(ui, if none.is_empty() { "No branches" } else { none });
                    }
                    let nodes =
                        build_tree(local.iter().copied(), |b: &LocalBranch| b.name.as_str());
                    tree(
                        ui,
                        c,
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
                                copy_name(ui, &b.name);
                                web_items(ui, &mut c.action, page.as_ref());
                                let items = local_branch_items(&b.name, &b.target, b.is_head, head);
                                menu(ui, &mut c.action, items);
                            });
                        },
                    );
                });
            }

            // Remotes.
            let count = remotes.iter().map(|r| r.2.len()).sum();
            let open = c.is_open("remotes");
            let (toggled, add) = section_header(
                ui,
                &mut c,
                "remotes",
                "Remote",
                count,
                open,
                Some("Add remote"),
            );
            if toggled {
                c.toggle("remotes");
            }
            if add {
                c.action = Some(Action::EditRemote(None));
            }
            if open {
                body(ui, &mut c, "remotes", "Remote", |ui, c| {
                    if remotes.is_empty() {
                        empty(ui, if none.is_empty() { "No remotes" } else { none });
                    }
                    for (name, url, branches) in &remotes {
                        let key = format!("remote:{name}");
                        let open = c.is_open(&key);
                        let response =
                            folder_row(ui, 0, icon::CLOUD, name, open, url.unwrap_or(name));
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
                                c,
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
                                        c.action =
                                            Some(Action::CheckoutRemote(b.full_name.clone()));
                                    } else if c.pick(ui, &response, me.clone()) {
                                    } else if response.clicked() {
                                        c.action = Some(Action::Jump(b.target.clone()));
                                    }
                                    drag_source(&response, &b.full_name);
                                    let page = web.branch(&b.full_name);
                                    response.context_menu(|ui| {
                                        if c.picked_menu(ui, &me, head) {
                                            return;
                                        }
                                        copy_name(ui, &b.full_name);
                                        web_items(ui, &mut c.action, page.as_ref());
                                        let items =
                                            remote_branch_items(&b.full_name, &b.target, head);
                                        menu(ui, &mut c.action, items);
                                    });
                                },
                            );
                        }
                    }
                });
            }

            // The forge's pull/merge requests and issues.
            if let Some(forge) = forge {
                forge_sections(ui, &mut c, forge, &matches);
            }

            // Tags.
            let open = c.is_open("tags");
            if section_header(ui, &mut c, "tags", "Tags", tags.len(), open, None).0 {
                c.toggle("tags");
            }
            if open {
                body(ui, &mut c, "tags", "Tags", |ui, c| {
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
                            copy_name(ui, &t.name);
                            web_items(ui, &mut c.action, page.as_ref());
                            menu(ui, &mut c.action, tag_items(&t.name, &t.target));
                        });
                    }
                });
            }

            // Stashes.
            let open = c.is_open("stashes");
            if section_header(ui, &mut c, "stashes", "Stashes", stashes.len(), open, None).0 {
                c.toggle("stashes");
            }
            if open {
                body(ui, &mut c, "stashes", "Stashes", |ui, c| {
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
                });
            }
            // Worktrees.
            let open = c.is_open("worktrees");
            let (toggled, add) = section_header(
                ui,
                &mut c,
                "worktrees",
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
                body(ui, &mut c, "worktrees", "Worktrees", |ui, c| {
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
                                    "Open a terminal here",
                                    !w.missing,
                                    Action::Terminal(w.path.clone()),
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
                });
            }

            // Submodules, if there are any.
            if !extras.submodules.is_empty() {
                let open = c.is_open("submodules");
                if section_header(
                    ui,
                    &mut c,
                    "submodules",
                    "Submodules",
                    extras.submodules.len(),
                    open,
                    None,
                )
                .0
                {
                    c.toggle("submodules");
                }
                if open {
                    body(ui, &mut c, "submodules", "Submodules", |ui, c| {
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
                                    (
                                        "Open a terminal here",
                                        cloned,
                                        Action::Terminal(full.clone()),
                                    ),
                                ];
                                menu(ui, &mut c.action, items);
                            });
                        }
                    });
                }
            }
        }
    });
    // Esc unpicks.
    if !c.picked.is_empty() && ui.input(|i| i.key_pressed(egui::Key::Escape)) {
        c.picked.clear();
    }
    ui.data_mut(|d| {
        d.insert_temp(picked_id, c.picked.clone());
        d.insert_temp(measured_id, c.sizes.measured.clone());
        d.insert_persisted(shares_id, c.sizes.shares.clone());
    });
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
