use crate::ports::inbound::user_command::UserCommandUseCase;

pub struct CliController<U>
where
    U: UserCommandUseCase,
{
    user_command_service: U,
}

impl<U> CliController<U>
where
    U: UserCommandUseCase,
{
    pub fn run(&self) {
        todo!()
    }
}
