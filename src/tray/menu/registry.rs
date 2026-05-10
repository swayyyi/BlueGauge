use std::collections::{HashMap, HashSet};
use std::hash::Hash;
use std::ops::Not;
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
    G: Clone + Copy + Eq + Hash + PartialEq + std::fmt::Debug,
{
    items: HashMap<Rc<MenuId>, MenuItemMeta<G>>,
    radio_groups: HashMap<G, RadioGroup>,
    checkbox_groups: HashMap<G, HashSet<Rc<MenuId>>>,
}

#[allow(dead_code)]
impl<G> MenuRegistry<G>
where
    G: Clone + Copy + Eq + Hash + PartialEq + std::fmt::Debug,
{
    pub fn new() -> Self {
        Self {
            items: HashMap::new(),
            radio_groups: HashMap::new(),
            checkbox_groups: HashMap::new(),
        }
    }

    pub fn register_normal(&mut self, kind: MenuItemKind) {
        let id = Rc::new(kind.id().clone());
        self.items.insert(id, MenuItemMeta { kind, group: None });
    }

    pub fn register_checkbox(&mut self, kind: MenuItemKind, group: G) -> bool {
        if kind.as_check_menuitem().is_none() {
            return false;
        }

        let id = Rc::new(kind.id().clone());

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
        kind: MenuItemKind,
        group: G,
        default: Option<MenuId>,
    ) -> bool {
        if kind.as_check_menuitem().is_none() {
            return false;
        }

        let id = Rc::new(kind.id().clone());

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
            .ok_or_else(|| format!("The menu not found: {id:?}"))?;

        let menu_group = menu_item_meta.group();

        let menu_kind = &menu_item_meta.kind();

        // Clicked menu is not in any group, return directly
        let Some(menu_group) = menu_group else {
            return Ok(menu_item_meta);
        };

        // Clicked menu is not in any [Radio] group, return directly
        let Some(radio_group) = self.radio_groups.get(menu_group) else {
            if self.checkbox_groups.contains_key(menu_group)
                && menu_kind.as_check_menuitem().is_some().not()
            {
                return Err(format!(
                    "Menu({id:?}) is not a [CheckMenuItem] on the checkbox group({menu_group:?})"
                ));
            }

            return Ok(menu_item_meta);
        };

        // <---Handle [Radio] menu--->
        let radio_menus_id = radio_group.members();

        let clickd_radio_menu_is_checked = menu_kind
            .as_check_menuitem()
            .ok_or_else(|| {
                format!("Menu({id:?}) is not a [CheckMenuItem] on the radio group({menu_group:?})")
            })?
            .is_checked();

        // Clicked menu is selected, deselect other raodio menus
        if clickd_radio_menu_is_checked {
            radio_menus_id
                .iter()
                .filter(|menu_id| menu_id.as_ref().ne(&id))
                .filter_map(|id| self.items.get(id))
                .filter_map(|menu_meta| menu_meta.kind().as_check_menuitem())
                .for_each(|check_menu| check_menu.set_checked(false));

            Ok(menu_item_meta)
        // Clicked menu is not selected, check if there is a default menu in the group
        } else {
            let Some(default_menu_id) = radio_group.default().as_ref() else {
                // No default menu, return and uncheck all menus
                self.get_radio_menu_from_group(menu_group)
                    .ok_or_else(|| format!("Failed to get radio menus from {menu_group:?}"))?
                    .iter()
                    .for_each(|check_menu| check_menu.set_checked(false));

                return Ok(menu_item_meta);
            };

            let default_menu_meta = self
                .items
                .get(default_menu_id)
                .ok_or_else(|| format!("Default menu({default_menu_id:?}) meta not found"))?;

            let default_menu_item = default_menu_meta.kind().as_check_menuitem()
                .ok_or_else(|| format!("Default Menu({default_menu_id:?}) is not a [CheckMenuItem] on the radio group({menu_group:?})"))?;

            // Uncheck all other menus except the default menu
            default_menu_item.set_checked(true);
            radio_menus_id
                .iter()
                .filter(|menu_id| menu_id.as_ref().ne(&default_menu_id))
                .filter_map(|id| self.items.get(id))
                .filter_map(|menu_meta| menu_meta.kind().as_check_menuitem())
                .for_each(|check_menu| check_menu.set_checked(false));

            Ok(default_menu_meta)
        }
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

    pub fn get_radio_id_from_group(&self, group: &G) -> Option<&HashSet<Rc<MenuId>>> {
        self.radio_groups.get(group).map(|r| r.members())
    }

    pub fn get_radio_menu_from_group(&self, group: &G) -> Option<Vec<&CheckMenuItem>> {
        self.get_radio_id_from_group(group).map(|ids| {
            ids.iter()
                .filter_map(|id| self.items.get(id))
                .filter_map(|meta| meta.kind.as_check_menuitem())
                .collect::<Vec<_>>()
        })
    }
}
