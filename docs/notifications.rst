
.. _notifications:

Notifications
=============

Proxmox Datacenter Manager emits notifications for noteworthy events, such as the result of a
scheduled job or a remote that could not be reached. This uses the same notification system as
Proxmox VE and Proxmox Backup Server: notification events are routed by *matchers* to one or more
*notification targets*.

The configuration is stored in ``/etc/proxmox-datacenter-manager/notifications.cfg``. Sensitive
data (such as passwords or tokens) is stored separately in
``/etc/proxmox-datacenter-manager/notifications-priv.cfg``, which is only readable by ``root``.

Notification Targets
---------------------

Proxmox Datacenter Manager offers the following notification target types:

:Sendmail: Uses the system's ``sendmail`` binary to send emails to a list of users or email
   addresses.

:SMTP: Sends emails directly via a configured SMTP server, optionally with authentication.

:Gotify: Sends a message to a `Gotify <https://gotify.net/>`_ server.

:Webhook: Sends a fully customizable HTTP request to an arbitrary URL. This can be used to
   integrate with services that are not natively supported, see :ref:`notifications_telegram`
   below for an example.

Notification Matchers
----------------------

A matcher routes a notification event to one or more targets. Matchers can have match rules to
selectively route notifications based on their metadata (for example severity or event type).
If no matcher matches a given notification, it is not delivered anywhere.

.. _notifications_telegram:

Telegram Notifications via Webhook
-----------------------------------

There is no dedicated Telegram notification target. Instead, a generic *Webhook* target can be
pointed directly at the Telegram Bot API to deliver messages to a Telegram chat:

#. Create a bot with `@BotFather <https://t.me/BotFather>`_ on Telegram and note down the bot
   token it gives you (looks like ``123456789:AAExampleTokenExampleTokenExample``).

#. Find out the chat ID you want to send messages to (for example by messaging your bot and
   checking ``https://api.telegram.org/bot<token>/getUpdates``).

#. Create a new Webhook notification target with:

   - **URL**: ``https://api.telegram.org/bot<token>/sendMessage``
   - **Method**: ``POST``
   - **Header**: ``Content-Type: application/json``
   - **Body**:

     .. code-block:: text

        {
          "chat_id": "<your-chat-id>",
          "text": "{{ escape "json" (subject) }}\n{{ escape "json" (body-text) }}"
        }

   Store the bot token as a secret of the webhook target and reference it from the URL as
   ``{{ secrets.token }}`` instead of pasting it in verbatim, so it does not appear in the public
   configuration file.

#. Add a matcher that routes the events you are interested in to this new target.
