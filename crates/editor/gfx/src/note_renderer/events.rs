use crate::note_renderer::NoteRenderer;

impl NoteRenderer {
    /// 处理音符事件队列
    pub fn process_events(
        &mut self,
        rx: &std::sync::mpsc::Receiver<crate::NoteEvent>,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
    ) -> bool {
        puffin::profile_function!();
        let mut updated = false;
        while let Ok(event) = rx.try_recv() {
            updated = true;
            match event {
                crate::NoteEvent::Reset(instances) => {
                    self.gpu_note_buffer.upload_all(&instances);
                }
                crate::NoteEvent::Add(instance) => {
                    self.gpu_note_buffer.add_note(&instance);
                }
                crate::NoteEvent::Update { index, instance } => {
                    self.gpu_note_buffer.update_note(index, &instance);
                }
                crate::NoteEvent::UpdateMany {
                    start_index,
                    instances,
                } => {
                    self.gpu_note_buffer.update_notes(start_index, &instances);
                }
                crate::NoteEvent::Remove(index) => {
                    self.gpu_note_buffer.remove_note(index);
                }
                crate::NoteEvent::RemoveAt { index, count } => {
                    self.gpu_note_buffer.remove_at(index, count);
                }
                crate::NoteEvent::Insert { index, instances } => {
                    self.gpu_note_buffer.insert_at(index, &instances);
                }
                crate::NoteEvent::Clear => {
                    self.gpu_note_buffer.clear();
                }
            }
        }
        if updated {
            self.update_cull_info(device, queue);
        }
        updated
    }
}
