
<img src="assets/logo-text-2.svg" width="100%" height="80px">

`Jar-po`
*Combination of Jar and Repo*

![License](https://img.shields.io/github/license/jacobtread/Jarpo?style=for-the-badge)
![NOT PRODUCTION READY](https://img.shields.io/badge/Not%20Ready%20For%20Production-ef4550?style=for-the-badge)

## 🔎 About

**Jarpo** is a work in progress website for hosting a collection of pre-compiled **Minecraft** server jar files. At this
point it is in very early progress and is not currently able to be used.

> This project is not yet usable as it is still under heavy development.


## 📃 Plan
**Jarpo** will be able to automatically detect when a new version of **Minecraft** is released and automatically download
and compile jars from providers such as Spigot, Paper, and the official minecraft server links.

## 🎀 Stack

This website is made up of two parts the **Backend** server which handles storing, ordering, and compiling of server Jars
and then the **Frontend** which is an interface to interact, browse, view, and download the stored jars

### 💻 Frontend

Currently not implemented but this stack will likely consist of **React** / **NextJS**, **SCSS**, and **Typescript**

### 🔧 Backend

The backend server is written in **Rust** but makes use of **Java** and **Maven** for compiling **Spigot** jars.
The web server itself will use the [Actix Web Framework (https://actix.rs/)](https://actix.rs/) for providing an API
interface for the frontend in the form of an HTTP REST API

## 🐢 Current Progress

### 📁 Spigot Build Tools

Currently, I am working on rewriting the spigot build tools in rust so that it is integrated into the 
backend directly and has a predictable execution as well as output. This also comes with performance
improvements. This is progressing steadily and has seen performance improvements from the addition
of asynchronous Rust

## 📜 License

Copyright (C) 2022  Jacobtread

This program is free software: you can redistribute it and/or modify
it under the terms of the GNU Affero General Public License as published
by the Free Software Foundation, version 3 of the License.

This program is distributed in the hope that it will be useful,
but WITHOUT ANY WARRANTY; without even the implied warranty of
MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
GNU Affero General Public License for more details.

You should have received a copy of the GNU Affero General Public License
along with this program.  If not, see <https://www.gnu.org/licenses/>.

### 📌 Additional Terms

Due to the nature of past events it is strictly off limits for any company or entity representing the 
Songoda company to use this software or any source code stored in this or related repositories and use
of said software.