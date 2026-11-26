use std::collections::{BTreeMap, HashMap};

use moca_protocol::{
    GameInterface, OptionDecl, OptionInputType, OptionKind, TaskDecl, TaskKind, TaskTier,
    parse_batch_input_items,
};

use crate::icons::{UiIcon, allocate_and_paint_icon};
use crate::theme::{IconButtonVariant, SegmentedOption, ThemeTokens, pill_badge};

pub enum SchemaAction {
    RunBatchTask(TaskDecl),
    ToggleTrigger {
        task_id: String,
        task: TaskDecl,
        active: bool,
    },
}

/// Render a clean, compact single-line task checklist (MaaEnd / MaaFwApp style).
pub fn render_task_list(
    ui: &mut egui::Ui,
    theme: &ThemeTokens,
    manifest: &GameInterface,
    selected_tasks: &mut Vec<String>,
    focused_task_id: &mut Option<String>,
    active_triggers: &HashMap<String, (bool, u32)>,
    connected: bool,
) -> Option<SchemaAction> {
    let mut action = None;

    let mut grouped: BTreeMap<String, Vec<(&String, &TaskDecl)>> = BTreeMap::new();
    for (task_id, task) in &manifest.task {
        if task.tier == TaskTier::Vip {
            continue;
        }
        let group_name = task.group.clone().unwrap_or_else(|| "默认任务".to_string());
        grouped.entry(group_name).or_default().push((task_id, task));
    }
    for tasks in grouped.values_mut() {
        tasks.sort_by(|a, b| a.0.cmp(b.0));
    }

    // Ensure a valid default focused task is always selected.
    if focused_task_id.as_ref().is_none_or(|id| {
        !manifest
            .task
            .get(id)
            .is_some_and(|t| t.tier != TaskTier::Vip)
    }) {
        let fallback = grouped
            .values()
            .flatten()
            .find(|(_, t)| t.kind == TaskKind::Trigger)
            .or_else(|| grouped.values().flatten().next())
            .map(|(id, _)| (*id).clone());
        *focused_task_id = fallback;
    }

    for (group_name, tasks) in grouped {
        ui.add_space(4.0);
        ui.label(
            egui::RichText::new(&group_name)
                .size(11.5)
                .strong()
                .color(theme.text_muted),
        );

        for (task_id, task) in tasks {
            let is_focused = focused_task_id.as_deref() == Some(task_id.as_str());
            let mut is_enabled = selected_tasks.contains(task_id);
            let (is_running, hit_count) =
                active_triggers.get(task_id).copied().unwrap_or((false, 0));

            let row_bg = if is_focused {
                theme.accent_soft_bg.gamma_multiply(0.85)
            } else {
                theme.bg_subcard
            };
            let row_border = if is_focused {
                egui::Stroke::new(1.0, theme.accent.gamma_multiply(0.48))
            } else {
                egui::Stroke::new(1.0, theme.border.gamma_multiply(0.35))
            };

            let frame_resp = egui::Frame::new()
                .fill(row_bg)
                .stroke(row_border)
                .corner_radius(egui::CornerRadius::same(theme.subcard_radius))
                .inner_margin(egui::Margin::symmetric(10, 6))
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.horizontal(|ui| {
                        if task.kind == TaskKind::Action {
                            allocate_and_paint_icon(
                                ui,
                                UiIcon::Bolt,
                                15.0,
                                if is_focused {
                                    theme.accent_hover
                                } else {
                                    theme.accent
                                },
                            );
                        } else {
                            let toggle_resp = theme.toggle_switch(ui, &mut is_enabled);
                            if toggle_resp.changed() {
                                if is_enabled {
                                    if !selected_tasks.contains(task_id) {
                                        selected_tasks.push(task_id.clone());
                                    }
                                } else {
                                    selected_tasks.retain(|id| id != task_id);
                                }
                                *focused_task_id = Some(task_id.clone());
                                if task.kind == TaskKind::Trigger {
                                    action = Some(SchemaAction::ToggleTrigger {
                                        task_id: task_id.clone(),
                                        task: task.clone(),
                                        active: is_enabled,
                                    });
                                }
                            }
                        }

                        let label_color = if is_focused {
                            theme.accent_hover
                        } else if task.kind == TaskKind::Action || is_enabled {
                            theme.text_primary
                        } else {
                            theme.text_secondary
                        };
                        let label_resp = ui.add(
                            egui::Label::new(
                                egui::RichText::new(&task.label)
                                    .size(13.0)
                                    .strong()
                                    .color(label_color),
                            )
                            .sense(egui::Sense::click()),
                        );
                        if label_resp.clicked() {
                            *focused_task_id = Some(task_id.clone());
                        }

                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if task.kind == TaskKind::Action {
                                pill_badge(
                                    ui,
                                    "一次性",
                                    theme.accent_hover,
                                    theme.accent_soft_bg,
                                    theme.accent.gamma_multiply(0.35),
                                );
                            } else if task.kind == TaskKind::Trigger {
                                if is_running {
                                    let txt = if hit_count > 0 {
                                        format!("● {hit_count}")
                                    } else {
                                        "● 监听".to_string()
                                    };
                                    pill_badge(
                                        ui,
                                        &txt,
                                        theme.success,
                                        theme.success_bg,
                                        theme.success.gamma_multiply(0.35),
                                    );
                                } else if is_enabled && !connected {
                                    pill_badge(
                                        ui,
                                        "待连接",
                                        theme.accent_hover,
                                        theme.accent_soft_bg,
                                        theme.accent.gamma_multiply(0.35),
                                    );
                                }
                            } else if !task.option.is_empty() {
                                allocate_and_paint_icon(
                                    ui,
                                    if is_focused {
                                        UiIcon::Sliders
                                    } else {
                                        UiIcon::Settings
                                    },
                                    13.5,
                                    if is_focused {
                                        theme.accent_hover
                                    } else {
                                        theme.text_muted
                                    },
                                );
                            }
                        });
                    });
                });

            if is_focused {
                let r = frame_resp.response.rect;
                let bar = egui::Rect::from_min_max(
                    egui::pos2(r.left() + 1.5, r.top() + 7.0),
                    egui::pos2(r.left() + 4.5, r.bottom() - 7.0),
                );
                ui.painter()
                    .rect_filled(bar, egui::CornerRadius::same(2), theme.accent_hover);
            }

            if frame_resp.response.interact(egui::Sense::click()).clicked() {
                *focused_task_id = Some(task_id.clone());
            }
        }
    }

    action
}

/// Render options and actions for the currently focused task in the inspector column.
#[allow(clippy::too_many_arguments)]
pub fn render_task_inspector(
    ui: &mut egui::Ui,
    theme: &ThemeTokens,
    manifest: &GameInterface,
    selected_tasks: &mut Vec<String>,
    focused_task_id: Option<&str>,
    option_values: &mut HashMap<String, serde_json::Value>,
    active_triggers: &HashMap<String, (bool, u32)>,
    connected: bool,
) -> Option<SchemaAction> {
    let Some(task_id) = focused_task_id else {
        ui.label(
            egui::RichText::new("请在左侧选择一个任务以查看详情和配置参数")
                .size(12.5)
                .color(theme.text_muted),
        );
        return None;
    };
    let task = manifest.task.get(task_id)?;

    let mut action = None;
    let mut is_enabled = selected_tasks.contains(&task_id.to_string());
    let (is_running, hit_count) = active_triggers.get(task_id).copied().unwrap_or((false, 0));

    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new(&task.label)
                .size(14.5)
                .strong()
                .color(theme.text_primary),
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if task.kind == TaskKind::Trigger {
                if is_running {
                    let txt = if hit_count > 0 {
                        format!("● 运行中 ({hit_count})")
                    } else {
                        "● 监听中".to_string()
                    };
                    pill_badge(
                        ui,
                        &txt,
                        theme.success,
                        theme.success_bg,
                        theme.success.gamma_multiply(0.35),
                    );
                } else if is_enabled && !connected {
                    pill_badge(
                        ui,
                        "已启用 · 待连接",
                        theme.accent_hover,
                        theme.accent_soft_bg,
                        theme.accent.gamma_multiply(0.4),
                    );
                } else {
                    pill_badge(
                        ui,
                        "未启用",
                        theme.text_muted,
                        theme.bg_subcard,
                        theme.border,
                    );
                }
            } else {
                let run_label = if task.kind == TaskKind::Action {
                    "立即执行"
                } else {
                    "单独执行"
                };
                if theme
                    .icon_button(
                        ui,
                        Some(UiIcon::Play),
                        run_label,
                        None,
                        IconButtonVariant::Primary,
                        26.0,
                        false,
                    )
                    .clicked()
                {
                    action = Some(SchemaAction::RunBatchTask(task.clone()));
                }
            }
        });
    });

    if let Some(desc) = &task.description {
        ui.add_space(2.0);
        ui.add(
            egui::Label::new(
                egui::RichText::new(desc)
                    .size(12.0)
                    .color(theme.text_secondary),
            )
            .wrap(),
        );
    }

    if task.kind != TaskKind::Action {
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            if theme.toggle_switch(ui, &mut is_enabled).changed() {
                let tid = task_id.to_string();
                if is_enabled {
                    if !selected_tasks.contains(&tid) {
                        selected_tasks.push(tid.clone());
                    }
                } else {
                    selected_tasks.retain(|id| id != &tid);
                }
                if task.kind == TaskKind::Trigger {
                    action = Some(SchemaAction::ToggleTrigger {
                        task_id: tid,
                        task: task.clone(),
                        active: is_enabled,
                    });
                }
            }
            let hint = match (task.kind, is_enabled) {
                (TaskKind::Trigger, true) => "已开启（连接窗口后自动运行并保存）",
                (TaskKind::Trigger, false) => "开启实时辅助监听",
                (TaskKind::Batch, true) => "已勾选加入批处理队列",
                (TaskKind::Batch, false) => "勾选加入批处理队列",
                (TaskKind::Action, _) => "",
            };
            ui.label(egui::RichText::new(hint).size(12.0).color(theme.text_muted));
        });
    }

    if !task.option.is_empty() {
        ui.add_space(4.0);
        ui.separator();
        ui.add_space(4.0);
        for opt_key in &task.option {
            if let Some(opt_decl) = manifest.option.get(opt_key) {
                render_option_widget(ui, theme, opt_key, opt_decl, option_values);
                ui.add_space(4.0);
            }
        }
    } else {
        ui.add_space(4.0);
        ui.label(
            egui::RichText::new("该任务当前无需额外配置参数")
                .size(11.5)
                .color(theme.text_muted),
        );
    }

    action
}

fn render_option_widget(
    ui: &mut egui::Ui,
    theme: &ThemeTokens,
    opt_key: &str,
    decl: &OptionDecl,
    values: &mut HashMap<String, serde_json::Value>,
) {
    let label = decl.label.as_deref().unwrap_or(opt_key);
    let current = values
        .entry(opt_key.to_string())
        .or_insert_with(|| decl.default.clone().unwrap_or(serde_json::Value::Null));

    match decl.kind {
        OptionKind::Boolean => {
            let mut val = current.as_bool().unwrap_or(false);
            ui.horizontal(|ui| {
                if theme.toggle_switch(ui, &mut val).changed() {
                    *current = serde_json::Value::Bool(val);
                }
                ui.label(
                    egui::RichText::new(label)
                        .size(12.5)
                        .color(theme.text_secondary),
                );
            });
        }
        OptionKind::Select => {
            if (2..=4).contains(&decl.items.len()) {
                ui.label(
                    egui::RichText::new(label)
                        .size(12.5)
                        .strong()
                        .color(theme.text_secondary),
                );
                ui.add_space(4.0);

                let entries: Vec<(String, serde_json::Value, bool, Option<&str>)> =
                    decl.items
                        .iter()
                        .map(|item| {
                            let item_val =
                                item.value.as_ref().cloned().unwrap_or_else(|| {
                                    serde_json::Value::String(item.name.clone())
                                });
                            let selected = &item_val == current;
                            let text = item.label.as_deref().unwrap_or(&item.name).to_string();
                            (text, item_val, selected, item.description.as_deref())
                        })
                        .collect();

                let selected_desc = entries
                    .iter()
                    .find(|(_, _, sel, _)| *sel)
                    .and_then(|(_, _, _, desc)| *desc);

                let all_compact = entries
                    .iter()
                    .all(|(text, _, _, _)| text.chars().count() <= 5);
                if all_compact {
                    let seg_options: Vec<SegmentedOption<'_>> = entries
                        .iter()
                        .map(|(text, _, sel, _)| SegmentedOption {
                            label: text.as_str(),
                            icon: None,
                            selected: *sel,
                        })
                        .collect();

                    if let Some(clicked_idx) = theme.segmented_row(ui, &seg_options)
                        && let Some((_, new_val, _, _)) = entries.get(clicked_idx)
                    {
                        *current = new_val.clone();
                    }

                    if let Some(desc) = selected_desc {
                        ui.add_space(4.0);
                        ui.add(
                            egui::Label::new(
                                egui::RichText::new(desc).size(11.5).color(theme.text_muted),
                            )
                            .wrap(),
                        );
                    }
                } else {
                    let radio_options: Vec<(&str, Option<&str>, bool)> = entries
                        .iter()
                        .map(|(text, _, sel, desc)| (text.as_str(), *desc, *sel))
                        .collect();
                    if let Some(clicked_idx) = theme.radio_list_group(ui, &radio_options)
                        && let Some((_, new_val, _, _)) = entries.get(clicked_idx)
                    {
                        *current = new_val.clone();
                    }
                }
                return;
            }

            let selected_text = decl
                .items
                .iter()
                .find(|item| {
                    let item_val = item
                        .value
                        .as_ref()
                        .cloned()
                        .unwrap_or_else(|| serde_json::Value::String(item.name.clone()));
                    &item_val == current
                })
                .map(|item| item.label.as_deref().unwrap_or(&item.name).to_string())
                .unwrap_or_else(|| "请选择".to_string());

            ui.label(
                egui::RichText::new(label)
                    .size(12.5)
                    .color(theme.text_secondary),
            );
            egui::ComboBox::from_id_salt(format!("combo_{opt_key}"))
                .width(ui.available_width())
                .selected_text(selected_text)
                .show_ui(ui, |ui| {
                    for item in &decl.items {
                        let item_val = item
                            .value
                            .as_ref()
                            .cloned()
                            .unwrap_or_else(|| serde_json::Value::String(item.name.clone()));
                        let text = item.label.as_deref().unwrap_or(&item.name);
                        if ui.selectable_label(&item_val == current, text).clicked() {
                            *current = item_val;
                        }
                    }
                });
        }
        OptionKind::Checkbox => {
            ui.label(
                egui::RichText::new(label)
                    .size(12.5)
                    .color(theme.text_secondary),
            );
            let mut arr = current.as_array().cloned().unwrap_or_default();
            ui.horizontal_wrapped(|ui| {
                for item in &decl.items {
                    let item_val = item
                        .value
                        .as_ref()
                        .cloned()
                        .unwrap_or_else(|| serde_json::Value::String(item.name.clone()));
                    let mut is_on = arr.contains(&item_val);
                    let text = item.label.as_deref().unwrap_or(&item.name);
                    if ui.checkbox(&mut is_on, text).changed() {
                        if is_on && !arr.contains(&item_val) {
                            arr.push(item_val);
                        } else if !is_on {
                            arr.retain(|v| v != &item_val);
                        }
                        *current = serde_json::Value::Array(arr.clone());
                    }
                }
            });
        }
        OptionKind::Input => {
            let mut text = current.as_str().unwrap_or_default().to_string();
            if decl.multiline {
                let count = parse_batch_input_items(&text).len();
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new(label)
                            .size(12.5)
                            .strong()
                            .color(theme.text_secondary),
                    );
                    pill_badge(
                        ui,
                        &format!("共 {count} 条"),
                        theme.accent_hover,
                        theme.accent_soft_bg,
                        theme.accent.gamma_multiply(0.4),
                    );
                    if let Some(default_str) = decl.default.as_ref().and_then(|v| v.as_str()) {
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.small_button("重置默认").clicked() {
                                text = default_str.to_string();
                                *current = serde_json::Value::String(text.clone());
                            }
                        });
                    }
                });
                ui.add_space(4.0);
                let frame = egui::Frame::new()
                    .fill(theme.bg_input)
                    .stroke(egui::Stroke::new(1.0, theme.border))
                    .corner_radius(egui::CornerRadius::same(theme.subcard_radius))
                    .inner_margin(egui::Margin::same(6));
                frame.show(ui, |ui| {
                    egui::ScrollArea::vertical()
                        .id_salt(("input_multiline_scroll", opt_key))
                        .max_height(138.0)
                        .show(ui, |ui| {
                            let mut edit = egui::TextEdit::multiline(&mut text)
                                .font(egui::TextStyle::Monospace)
                                .desired_width(ui.available_width())
                                .desired_rows(6)
                                .frame(egui::Frame::NONE);
                            if let Some(hint) = &decl.placeholder {
                                edit = edit.hint_text(hint);
                            }
                            if ui.add(edit).changed() {
                                *current = serde_json::Value::String(text);
                            }
                        });
                });
                if let Some(desc) = &decl.description {
                    ui.add_space(3.0);
                    ui.add(
                        egui::Label::new(
                            egui::RichText::new(desc).size(11.5).color(theme.text_muted),
                        )
                        .wrap(),
                    );
                }
            } else {
                ui.label(
                    egui::RichText::new(label)
                        .size(12.5)
                        .color(theme.text_secondary),
                );
                let mut edit =
                    egui::TextEdit::singleline(&mut text).desired_width(ui.available_width());
                if let Some(hint) = &decl.placeholder {
                    edit = edit.hint_text(hint);
                }
                if decl.input_type == OptionInputType::Password {
                    edit = edit.password(true);
                }
                if ui.add(edit).changed() {
                    *current = serde_json::Value::String(text);
                }
            }
        }
    }
}
