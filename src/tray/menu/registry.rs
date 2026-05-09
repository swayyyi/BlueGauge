use std::collections::{HashMap, HashSet};
use std::hash::Hash;
use std::rc::Rc;

use getset::Getters;
use tray_icon::menu::{CheckMenuItem, MenuId, MenuItemKind};

#[derive(Clone, Getters)]
#[getset(get = "pub")]
pub struct MenuItemMeta<G> {
    kind: MenuItemKind,
    group: Option<G>,
}

impl<G> PartialEq for MenuItemMeta<G>
where
    G: PartialEq,
{
    fn eq(&self, other: &Self) -> bool {
        self.group == other.group && self.kind.id() == other.kind.id()
    }
}

#[derive(Clone, Getters)]
#[getset(get = "pub")]
struct RadioGroup {
    members: HashSet<Rc<MenuId>>,
    default: Option<MenuId>,
}

#[derive(Clone)]
pub struct MenuRegistry<G>
where
    G: Clone + Copy + Eq + Hash + PartialEq,
{
    items: HashMap<Rc<MenuId>, MenuItemMeta<G>>,
    radio_groups: HashMap<G, RadioGroup>,
    checkbox_groups: HashMap<G, HashSet<Rc<MenuId>>>,
}

#[allow(dead_code)]
impl<G> MenuRegistry<G>
where
    G: Clone + Copy + Eq + Hash + PartialEq,
{
    pub fn new() -> Self {
        Self {
            items: HashMap::new(),
            radio_groups: HashMap::new(),
            checkbox_groups: HashMap::new(),
        }
    }

    pub fn register_normal(&mut self, id: MenuId, kind: MenuItemKind) {
        self.items
            .insert(Rc::new(id), MenuItemMeta { kind, group: None });
    }

    pub fn register_checkbox(&mut self, id: MenuId, kind: MenuItemKind, group: G) -> bool {
        if kind.as_check_menuitem().is_none() {
            return false;
        }

        let id = Rc::new(id);

        self.items.insert(
            id.clone(),
            MenuItemMeta {
                kind,
                group: Some(group),
            },
        );

        self.checkbox_groups.entry(group).or_default().insert(id);

        true
    }

    pub fn register_radio(
        &mut self,
        id: MenuId,
        kind: MenuItemKind,
        group: G,
        default: Option<MenuId>,
    ) -> bool {
        if kind.as_check_menuitem().is_none() {
            return false;
        }

        let id = Rc::new(id);

        self.items.insert(
            id.clone(),
            MenuItemMeta {
                kind,
                group: Some(group),
            },
        );

        self.radio_groups
            .entry(group)
            .or_insert_with(|| RadioGroup {
                members: HashSet::new(),
                default,
            })
            .members
            .insert(id);

        true
    }

    pub fn deregister_normal(&mut self, id: &MenuId) -> bool {
        self.items.remove(id).is_some()
    }

    pub fn deregister_checkbox(&mut self, id: &MenuId, group: G) -> bool {
        self.items.remove(id);

        self.checkbox_groups
            .get_mut(&group)
            .map(|checkbox_group| checkbox_group.remove(id))
            .unwrap_or_default()
    }

    pub fn deregister_radio(&mut self, id: &MenuId, group: G) -> bool {
        self.items.remove(id);

        self.radio_groups
            .get_mut(&group)
            .map(|radio_group| radio_group.members.remove(id))
            .unwrap_or_default()
    }

    pub fn handle_event(&mut self, id: &MenuId) -> Result<&MenuItemMeta<G>, String> {
        let menu_item_meta = self
            .items
            .get(id)
            .ok_or(format!("Menu item not found: {id:?}"))?;

        let menu_group = menu_item_meta.group;

        let menu_kind = &menu_item_meta.kind;

        if let Some(menu_group) = menu_group {
            // 处理单选框组
            if let Some(radio_group) = self.radio_groups.get(&menu_group) {
                let Some(click_menu) = menu_kind.as_check_menuitem() else {
                    return Err(format!(
                        "Clicked menu is not a CheckMenu on a radio group: {id:?}"
                    ));
                };

                let click_menu_state = click_menu.is_checked();

                if click_menu_state {
                    // 点击菜单选中，其余菜单取消选中
                    self.get_radio_id_from_group(menu_group)
                        .ok_or(format!("Failed to get radio id from {id:?}"))
                        .map(|ids| {
                            ids.iter()
                                .filter(|menu_id| menu_id.as_ref().ne(&id))
                                .filter_map(|id| self.items.get(id))
                                .filter_map(|meta| meta.kind.as_check_menuitem())
                                .for_each(|check_menu| {
                                    check_menu.set_checked(false);
                                });
                        })?;

                    return Ok(menu_item_meta);
                } else {
                    // 点击的菜单未选中时
                    let Some(default_menu_id) = radio_group.default.as_ref() else {
                        // 无默认菜单时返回，全部菜单取消选中
                        self.get_radio_menu_from_group(menu_group)
                            .ok_or(format!("Failed to get radio id from {id:?}"))
                            .map(|ids| {
                                ids.iter().for_each(|check_menu| {
                                    check_menu.set_checked(false);
                                });
                            })?;
                        return Ok(menu_item_meta);
                    };
                    let default_menu_meta = self.items.get(default_menu_id);

                    let Some(default_menu_meta) = default_menu_meta else {
                        return Err(format!("Default menu item not found: {default_menu_id:?}"));
                    };

                    if let Some(default_menu) = default_menu_meta.kind.as_check_menuitem() {
                        // 默认菜单选中，其余菜单取消选中
                        default_menu.set_checked(true);
                        self.get_radio_id_from_group(menu_group)
                            .ok_or(format!("Failed to get radio id from {id:?}"))
                            .map(|ids| {
                                ids.iter()
                                    .filter(|menu_id| menu_id.as_ref().ne(&default_menu_id))
                                    .filter_map(|id| self.items.get(id))
                                    .filter_map(|meta| meta.kind.as_check_menuitem())
                                    .for_each(|check_menu| {
                                        check_menu.set_checked(false);
                                    });
                            })?;
                        return Ok(default_menu_meta);
                    } else {
                        return Err(format!(
                            "Default menu item is not a CheckMenu: {default_menu_id:?}"
                        ));
                    }
                };
            }

            // 处理复选框组
            self.checkbox_groups
                .contains_key(&menu_group)
                .then_some(())
                .ok_or(format!("Menu item not found in checkbox group: {id:?}"))?;
        }

        Ok(menu_item_meta)
    }

    pub fn get_menu_meta_from_id(&self, id: &MenuId) -> Option<&MenuItemMeta<G>> {
        self.items.get(id)
    }

    pub fn get_menu_kind_from_id(&self, id: &MenuId) -> Option<&MenuItemKind> {
        self.items.get(id).map(|meta| &meta.kind)
    }

    pub fn get_menu_group_from_id(&self, id: &MenuId) -> Option<G> {
        self.items.get(id).and_then(|meta| meta.group)
    }

    pub fn get_checkbox_id_from_group(&self, group: G) -> Option<&HashSet<Rc<MenuId>>> {
        self.checkbox_groups.get(&group)
    }

    pub fn get_checkbox_menu_from_group(&self, group: G) -> Option<Vec<&CheckMenuItem>> {
        self.get_checkbox_id_from_group(group).map(|ids| {
            ids.iter()
                .filter_map(|id| self.items.get(id))
                .filter_map(|meta| meta.kind.as_check_menuitem())
                .collect::<Vec<_>>()
        })
    }

    pub fn get_radio_id_from_group(&self, group: G) -> Option<&HashSet<Rc<MenuId>>> {
        self.radio_groups.get(&group).map(|r| r.members())
    }

    pub fn get_radio_menu_from_group(&self, group: G) -> Option<Vec<&CheckMenuItem>> {
        self.get_radio_id_from_group(group).map(|ids| {
            ids.iter()
                .filter_map(|id| self.items.get(id))
                .filter_map(|meta| meta.kind.as_check_menuitem())
                .collect::<Vec<_>>()
        })
    }
}
