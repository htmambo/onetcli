//! MongoDB 树形视图事件处理

use gpui::{AppContext, Context, Entity, Subscription, Window};
use gpui_component::table::TableEvent;

use crate::MongoNodeType;
use crate::collection_view::{CollectionTabConfig, CollectionTabView, CollectionView};
use crate::mongo_tree_view::{MongoTreeView, MongoTreeViewEvent};
use one_core::tab_container::{TabContainer, TabItem};

/// MongoDB 事件处理器
pub struct MongoEventHandler {
    _tree_subscription: Subscription,
    _table_subscription: Subscription,
}

impl MongoEventHandler {
    pub fn new(
        tree_view: &Entity<MongoTreeView>,
        tab_container: Entity<TabContainer>,
        collection_view: Entity<CollectionView>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let tree_view_clone = tree_view.clone();
        let collection_view_clone = collection_view.clone();
        let tab_container_clone = tab_container.clone();

        let tree_subscription = cx.subscribe_in(
            tree_view,
            window,
            move |_handler, _tree, event, window, cx| {
                let tree_view = tree_view_clone.clone();
                let collection_view = collection_view_clone.clone();
                let tab_container = tab_container_clone.clone();

                match event {
                    MongoTreeViewEvent::CollectionSelected { node_id } => {
                        if let Some(node) = tree_view.read(cx).get_node(node_id).cloned() {
                            if node.node_type == MongoNodeType::Collection {
                                let Some(database_name) = node.database_name.clone() else {
                                    return;
                                };
                                let Some(collection_name) = node.collection_name.clone() else {
                                    return;
                                };
                                collection_view.update(cx, |view, cx| {
                                    view.load_collection(
                                        node.connection_id.clone(),
                                        database_name,
                                        collection_name,
                                        cx,
                                    );
                                });
                            }
                        }
                    }
                    MongoTreeViewEvent::CollectionOpenInTab { node_id } => {
                        if let Some(node) = tree_view.read(cx).get_node(node_id).cloned() {
                            if node.node_type == MongoNodeType::Collection {
                                let Some(database_name) = node.database_name.clone() else {
                                    return;
                                };
                                let Some(collection_name) = node.collection_name.clone() else {
                                    return;
                                };
                                let tab_id = format!(
                                    "mongo-collection:{}:{}:{}",
                                    node.connection_id, database_name, collection_name
                                );
                                let config = CollectionTabConfig {
                                    connection_id: node.connection_id.clone(),
                                    database_name: database_name.clone(),
                                    collection_name: collection_name.clone(),
                                };
                                tab_container.update(cx, |container, cx| {
                                    let tab_id_clone = tab_id.clone();
                                    let config_clone = config.clone();
                                    container.activate_or_add_tab_lazy(
                                        tab_id,
                                        move |window, cx| {
                                            let view = cx.new(|cx| {
                                                CollectionTabView::new(config_clone, window, cx)
                                            });
                                            TabItem::new(tab_id_clone, "mongodb", view)
                                        },
                                        window,
                                        cx,
                                    );
                                });
                            }
                        }
                    }
                    MongoTreeViewEvent::DatabaseSelected { node_id } => {
                        if let Some(node) = tree_view.read(cx).get_node(node_id).cloned() {
                            if node.node_type == MongoNodeType::Database {
                                let Some(database_name) = node.database_name.clone() else {
                                    return;
                                };
                                collection_view.update(cx, |view, cx| {
                                    view.load_database_collections(
                                        node.connection_id.clone(),
                                        database_name,
                                        cx,
                                    );
                                });
                            }
                        }
                    }
                    MongoTreeViewEvent::ConnectionEstablished { .. } => {}
                }
            },
        );

        let collection_view_for_table = collection_view.clone();
        let tab_container_for_table = tab_container.clone();
        let table_subscription = cx.subscribe_in(
            &collection_view.read(cx).table_entity(),
            window,
            move |_handler, _table, event: &TableEvent, window, cx| {
                if let TableEvent::DoubleClickedRow(row_ix) = event {
                    let collection_view = collection_view_for_table.clone();
                    let tab_container = tab_container_for_table.clone();
                    let item = collection_view
                        .read(cx)
                        .database_collection_at(*row_ix)
                        .cloned();
                    let Some(item) = item else { return };
                    let Some(connection_id) = collection_view.read(cx).current_connection_id().cloned() else { return };
                    let Some(database_name) = collection_view.read(cx).current_database_name().cloned() else { return };
                    let collection_name = item.name;
                    let tab_id = format!(
                        "mongo-collection:{}:{}:{}",
                        connection_id, database_name, collection_name
                    );
                    let config = CollectionTabConfig {
                        connection_id: connection_id.clone(),
                        database_name: database_name.clone(),
                        collection_name: collection_name.clone(),
                    };
                    tab_container.update(cx, |container, cx| {
                        let tab_id_clone = tab_id.clone();
                        let config_clone = config.clone();
                        container.activate_or_add_tab_lazy(
                            tab_id,
                            move |window, cx| {
                                let view = cx.new(|cx| {
                                    CollectionTabView::new(config_clone, window, cx)
                                });
                                TabItem::new(tab_id_clone, "mongodb", view)
                            },
                            window,
                            cx,
                        );
                    });
                }
            },
        );

        Self {
            _tree_subscription: tree_subscription,
            _table_subscription: table_subscription,
        }
    }
}
