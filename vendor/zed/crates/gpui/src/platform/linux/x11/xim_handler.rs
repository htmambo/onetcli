use std::{default::Default, env};

use x11rb::protocol::{Event, xproto};
use xim::{AHashMap, AttributeName, Client, ClientError, ClientHandler, InputStyle};

pub enum XimCallbackEvent {
    XimXEvent(x11rb::protocol::Event),
    XimPreeditEvent(xproto::Window, String),
    XimCommitEvent(xproto::Window, String),
}

pub struct XimHandler {
    pub im_id: u16,
    pub ic_id: u16,
    pub connected: bool,
    pub window: xproto::Window,
    pub last_callback_event: Option<XimCallbackEvent>,
}

impl XimHandler {
    pub fn new() -> Self {
        Self {
            im_id: Default::default(),
            ic_id: Default::default(),
            connected: false,
            window: Default::default(),
            last_callback_event: None,
        }
    }
}

fn preferred_xim_locale_from_candidates<'a>(
    candidates: impl IntoIterator<Item = Option<&'a str>>,
) -> &'a str {
    candidates
        .into_iter()
        .flatten()
        .map(str::trim)
        .find(|locale| !locale.is_empty() && !matches!(*locale, "C" | "POSIX"))
        .unwrap_or("C")
}

fn preferred_xim_locale() -> String {
    preferred_xim_locale_from_candidates([
        env::var("LC_CTYPE").ok().as_deref(),
        env::var("LC_ALL").ok().as_deref(),
        env::var("LANG").ok().as_deref(),
    ])
    .to_string()
}

impl<C: Client<XEvent = xproto::KeyPressEvent>> ClientHandler<C> for XimHandler {
    fn handle_connect(&mut self, client: &mut C) -> Result<(), ClientError> {
        let locale = preferred_xim_locale();
        log::info!("XIM: opening input method with locale {}", locale);
        client.open(&locale)
    }

    fn handle_open(&mut self, client: &mut C, input_method_id: u16) -> Result<(), ClientError> {
        self.im_id = input_method_id;

        client.get_im_values(input_method_id, &[AttributeName::QueryInputStyle])
    }

    fn handle_get_im_values(
        &mut self,
        client: &mut C,
        input_method_id: u16,
        _attributes: AHashMap<AttributeName, Vec<u8>>,
    ) -> Result<(), ClientError> {
        let ic_attributes = client
            .build_ic_attributes()
            .push(AttributeName::InputStyle, InputStyle::PREEDIT_CALLBACKS)
            .push(AttributeName::ClientWindow, self.window)
            .push(AttributeName::FocusWindow, self.window)
            .build();
        client.create_ic(input_method_id, ic_attributes)
    }

    fn handle_create_ic(
        &mut self,
        _client: &mut C,
        _input_method_id: u16,
        input_context_id: u16,
    ) -> Result<(), ClientError> {
        self.connected = true;
        self.ic_id = input_context_id;
        Ok(())
    }

    fn handle_commit(
        &mut self,
        _client: &mut C,
        _input_method_id: u16,
        _input_context_id: u16,
        text: &str,
    ) -> Result<(), ClientError> {
        self.last_callback_event = Some(XimCallbackEvent::XimCommitEvent(
            self.window,
            String::from(text),
        ));
        Ok(())
    }

    fn handle_forward_event(
        &mut self,
        _client: &mut C,
        _input_method_id: u16,
        _input_context_id: u16,
        _flag: xim::ForwardEventFlag,
        xev: C::XEvent,
    ) -> Result<(), ClientError> {
        match xev.response_type {
            x11rb::protocol::xproto::KEY_PRESS_EVENT => {
                self.last_callback_event = Some(XimCallbackEvent::XimXEvent(Event::KeyPress(xev)));
            }
            x11rb::protocol::xproto::KEY_RELEASE_EVENT => {
                self.last_callback_event =
                    Some(XimCallbackEvent::XimXEvent(Event::KeyRelease(xev)));
            }
            _ => {}
        }
        Ok(())
    }

    fn handle_close(&mut self, client: &mut C, _input_method_id: u16) -> Result<(), ClientError> {
        client.disconnect()
    }

    fn handle_preedit_draw(
        &mut self,
        _client: &mut C,
        _input_method_id: u16,
        _input_context_id: u16,
        _caret: i32,
        _chg_first: i32,
        _chg_len: i32,
        _status: xim::PreeditDrawStatus,
        preedit_string: &str,
        _feedbacks: Vec<xim::Feedback>,
    ) -> Result<(), ClientError> {
        // XIMReverse: 1, XIMPrimary: 8, XIMTertiary: 32: selected text
        // XIMUnderline: 2, XIMSecondary: 16: underlined text
        // XIMHighlight: 4: normal text
        // XIMVisibleToForward: 64, XIMVisibleToBackward: 128, XIMVisibleCenter: 256: text align position
        // XIMPrimary, XIMHighlight, XIMSecondary, XIMTertiary are not specified,
        // but interchangeable as above
        // Currently there's no way to support these.
        self.last_callback_event = Some(XimCallbackEvent::XimPreeditEvent(
            self.window,
            String::from(preedit_string),
        ));
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::preferred_xim_locale_from_candidates;

    #[test]
    fn prefers_valid_lc_ctype_locale() {
        let locale = preferred_xim_locale_from_candidates([
            Some("zh_CN.UTF-8"),
            Some("en_US.UTF-8"),
            Some("C"),
        ]);

        assert_eq!(locale, "zh_CN.UTF-8");
    }

    #[test]
    fn skips_c_locale_and_keeps_fallback_chain() {
        let locale =
            preferred_xim_locale_from_candidates([Some("C"), Some("POSIX"), Some("C.UTF-8")]);

        assert_eq!(locale, "C.UTF-8");
    }

    #[test]
    fn falls_back_to_c_when_no_locale_is_available() {
        let locale = preferred_xim_locale_from_candidates([None, Some(""), Some("C")]);

        assert_eq!(locale, "C");
    }
}
