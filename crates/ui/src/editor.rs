use iced_widget::space;

use super::Element;

pub struct Editor {

}

impl Editor {
    pub fn new() -> Self {
        Self {

        }
    }

    pub fn view(&self) -> Element<'_> {
        space().into()
    }
}
