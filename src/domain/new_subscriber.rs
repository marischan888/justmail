use crate::domain::subscriber_name::SubscriberName;
use crate::domain::SubscriberEmail;

pub struct NewSubscriber {
    pub name: SubscriberName,
    pub email: SubscriberEmail,
}
